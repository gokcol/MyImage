use super::*;
use crate::{capture::CaptureMode, scroll_capture::Stitcher};

#[derive(Default)]
pub(super) struct RegionDrag {
    start: Option<Pos2>,
    end: Option<Pos2>,
    active: bool,
    adjustment: Option<(Pos2, Rect, [bool; 4])>,
}
impl RegionDrag {
    fn update_window(
        &mut self,
        events: &[egui::Event],
        bounds: Rect,
        controls: Rect,
        windows: &[(u32, Rect)],
        selected: &mut Option<u32>,
    ) {
        for event in events {
            if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } = event
                && bounds.contains(*pos)
                && !controls.contains(*pos)
                && self.bounds().is_none_or(|r| !r.expand(7.).contains(*pos))
            {
                *selected = windows
                    .iter()
                    .find(|(_, r)| r.contains(*pos))
                    .map(|(id, _)| *id);
                *self = Self::default();
            }
            let Some((_, target)) = windows.iter().find(|(id, _)| Some(*id) == *selected) else {
                continue;
            };
            let target = target.intersect(bounds);
            let click = matches!(event, egui::Event::PointerButton {pos, button: egui::PointerButton::Primary, pressed: false, ..}
                if self.active && self.adjustment.is_none() && self.start.is_some_and(|a| a.distance(*pos) < 3.));
            self.update(std::slice::from_ref(event), target, controls);
            if click {
                self.start = Some(target.min);
                self.end = Some(target.max);
            }
        }
    }
    fn update(&mut self, events: &[egui::Event], bounds: Rect, controls: Rect) {
        for event in events {
            match *event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if bounds.contains(pos) && !controls.contains(pos) => {
                    self.adjustment = self.bounds().and_then(|rect| {
                        let edges = Self::edges(rect, pos);
                        (edges.iter().any(|v| *v) || rect.contains(pos))
                            .then_some((pos, rect, edges))
                    });
                    if self.adjustment.is_none() {
                        self.start = Some(bounds.clamp(pos));
                        self.end = None;
                    }
                    self.active = true;
                }
                egui::Event::PointerMoved(pos) if self.active => self.move_to(pos, bounds),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } if self.active => {
                    self.move_to(pos, bounds);
                    self.active = false;
                    self.adjustment = None;
                }
                egui::Event::PointerGone => self.active = false,
                _ => {}
            }
        }
    }
    fn edges(rect: Rect, pos: Pos2) -> [bool; 4] {
        if !rect.expand(7.).contains(pos) {
            return [false; 4];
        }
        [
            (pos.x - rect.left()).abs() <= 7.,
            (pos.x - rect.right()).abs() <= 7.,
            (pos.y - rect.top()).abs() <= 7.,
            (pos.y - rect.bottom()).abs() <= 7.,
        ]
    }
    fn cursor(&self, pos: Pos2) -> egui::CursorIcon {
        let Some(rect) = self.bounds() else {
            return egui::CursorIcon::Crosshair;
        };
        let [l, r, t, b] = Self::edges(rect, pos);
        match (l || r, t || b) {
            (true, true) if (l && t) || (r && b) => egui::CursorIcon::ResizeNwSe,
            (true, true) => egui::CursorIcon::ResizeNeSw,
            (true, false) => egui::CursorIcon::ResizeHorizontal,
            (false, true) => egui::CursorIcon::ResizeVertical,
            _ if rect.contains(pos) => egui::CursorIcon::Move,
            _ => egui::CursorIcon::Crosshair,
        }
    }
    fn move_to(&mut self, pos: Pos2, bounds: Rect) {
        if let Some((origin, rect, [l, r, t, b])) = self.adjustment {
            let delta = pos - origin;
            let target = if l || r || t || b {
                Rect::from_min_max(
                    Pos2::new(
                        if l {
                            pos.x.clamp(bounds.left(), rect.right() - 1.)
                        } else {
                            rect.left()
                        },
                        if t {
                            pos.y.clamp(bounds.top(), rect.bottom() - 1.)
                        } else {
                            rect.top()
                        },
                    ),
                    Pos2::new(
                        if r {
                            pos.x.clamp(rect.left() + 1., bounds.right())
                        } else {
                            rect.right()
                        },
                        if b {
                            pos.y.clamp(rect.top() + 1., bounds.bottom())
                        } else {
                            rect.bottom()
                        },
                    ),
                )
            } else {
                rect.translate(Vec2::new(
                    delta
                        .x
                        .clamp(bounds.left() - rect.left(), bounds.right() - rect.right()),
                    delta
                        .y
                        .clamp(bounds.top() - rect.top(), bounds.bottom() - rect.bottom()),
                ))
            };
            self.start = Some(target.min);
            self.end = Some(target.max);
        } else {
            self.end = Some(bounds.clamp(pos));
        }
    }
    fn bounds(&self) -> Option<Rect> {
        self.start
            .zip(self.end)
            .map(|(a, b)| Rect::from_two_pos(a, b))
            .filter(|r| r.width() >= 1. && r.height() >= 1.)
    }
}

pub(super) struct CapturedFrame {
    pub image: image::RgbaImage,
    pub windows: Vec<capture::WindowInfo>,
}
pub(super) struct ScrollSession {
    display: DisplayInfo,
    rect: selection::PixelRect,
    capture_size: (u32, u32),
    captured_at: DateTime<Local>,
    pub stitcher: Option<Stitcher>,
    pub message: String,
    pub texture: Option<egui::TextureHandle>,
    pub window_id: Option<u32>,
    window_crop: Option<selection::PixelRect>,
    pub paused: bool,
    pub finish_requested: bool,
    pub next_capture: std::time::Instant,
    placement_frames: u8,
    demo_offset: u32,
}

impl MyImageApp {
    pub(super) fn save_captured_image(
        &mut self,
        ctx: &egui::Context,
        image: image::RgbaImage,
        at: DateTime<Local>,
    ) {
        self.busy = true;
        self.view = View::Editor;
        self.status = "Saving capture…".into();
        let folder = self.settings.output_dir.clone();
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = storage::save_capture(&image, &folder, at);
            let _ = tx.send(WorkEvent::Saved(Arc::new(image), path, true));
            ctx.request_repaint();
        });
    }

    pub(super) fn selection_ui(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_selection(ctx);
            return;
        }
        let Some(selection) = &mut self.selection else {
            return;
        };
        let placing_overlay = selection.placement_frames < 4;
        if placing_overlay {
            let divisor = if cfg!(target_os = "windows") {
                ctx.pixels_per_point()
            } else {
                ctx.zoom_factor()
            };
            let display = &selection.display;
            if selection.placement_frames == 0 {
                if cfg!(target_os = "windows") {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
                }
                ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::ZERO));
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            }
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                display.width as f32 / divisor,
                display.height as f32 / divisor,
            )));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
                display.x as f32 / divisor,
                display.y as f32 / divisor,
            )));
            selection.placement_frames += 1;
            ctx.request_repaint();
        }
        // Resizing/moving the editor into a screen overlay invalidates its old
        // window-relative hover coordinates. Wait for an event in the new frame.
        if !placing_overlay
            && ctx.input(|i| {
                i.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }
                    )
                })
            })
        {
            selection.pointer_ready = true;
        }
        let mode = self.settings.capture_mode;
        let mut accepted = None;
        let mut cancel = false;
        let mut window = None;
        // Place the controller before the canvas so its hit region takes priority.
        let bar = egui::Area::new(egui::Id::new("capture-now-controls"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0., -24.))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(appearance::NAVY)
                    .corner_radius(12)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 10.;
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Capture Now  ·  {}", mode.label()))
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.checkbox(
                                &mut selection.magnifier,
                                egui::RichText::new("Magnifier").color(Color32::WHITE),
                            );
                            if ui.button("Reset").clicked() {
                                selection.drag = RegionDrag::default();
                                selection.window_id = None;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel = true;
                            }
                            let ready = selection.drag.bounds().is_some();
                            if ui
                                .add_enabled(
                                    ready,
                                    appearance::primary(if mode == CaptureMode::Scrolling {
                                        "Start scrolling"
                                    } else {
                                        "Capture"
                                    }),
                                )
                                .clicked()
                            {
                                accepted = Some(selection::PixelRect {
                                    x: 0,
                                    y: 0,
                                    width: 0,
                                    height: 0,
                                });
                            }
                        });
                    });
            })
            .response
            .rect;
        let confirm = accepted.take().is_some() || ctx.input(|i| i.key_pressed(egui::Key::Enter));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let bounds = ui.max_rect();
                let response = ui.allocate_rect(bounds, egui::Sense::click_and_drag());
                let pointer = ctx
                    .input(|i| i.pointer.hover_pos())
                    .filter(|p| selection.pointer_ready && bounds.contains(*p));
                if response.hovered() && mode != CaptureMode::Window {
                    ctx.set_cursor_icon(
                        pointer.map_or(egui::CursorIcon::Crosshair, |p| selection.drag.cursor(p)),
                    );
                }
                let window_rect = |w: &capture::WindowInfo| {
                    Rect::from_min_size(
                        bounds.min
                            + Vec2::new(
                                (w.x - selection.display.x) as f32 / selection.display.width as f32
                                    * bounds.width(),
                                (w.y - selection.display.y) as f32
                                    / selection.display.height as f32
                                    * bounds.height(),
                            ),
                        Vec2::new(
                            w.width as f32 / selection.display.width as f32 * bounds.width(),
                            w.height as f32 / selection.display.height as f32 * bounds.height(),
                        ),
                    )
                };
                let hovered_window = if matches!(mode, CaptureMode::Window | CaptureMode::Scrolling)
                {
                    pointer.filter(|p| !bar.contains(*p)).and_then(|p| {
                        selection
                            .windows
                            .iter()
                            .find(|w| window_rect(w).contains(p))
                    })
                } else {
                    None
                };
                if mode == CaptureMode::Window {
                    if response.clicked()
                        && let Some(w) = hovered_window
                    {
                        let rect = window_rect(w).intersect(bounds);
                        selection.drag.start = Some(rect.min);
                        selection.drag.end = Some(rect.max);
                        selection.window_id = Some(w.id);
                    }
                } else if mode == CaptureMode::Scrolling {
                    let windows: Vec<_> = selection
                        .windows
                        .iter()
                        .map(|w| (w.id, window_rect(w)))
                        .collect();
                    selection.drag.update_window(
                        &ctx.input(|i| i.events.clone()),
                        bounds,
                        bar,
                        &windows,
                        &mut selection.window_id,
                    );
                } else {
                    selection
                        .drag
                        .update(&ctx.input(|i| i.events.clone()), bounds, bar);
                }
                let chosen = selection.drag.bounds().map(|r| r.intersect(bounds));
                let selected =
                    chosen.or_else(|| hovered_window.map(|w| window_rect(w).intersect(bounds)));
                let painter = ui.painter();
                painter.image(
                    selection.texture.id(),
                    bounds,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                    Color32::WHITE,
                );
                let shade = Color32::from_black_alpha(145);
                if let Some(rect) = selected {
                    for outside in [
                        Rect::from_min_max(bounds.min, Pos2::new(bounds.right(), rect.top())),
                        Rect::from_min_max(Pos2::new(bounds.left(), rect.bottom()), bounds.max),
                        Rect::from_min_max(
                            Pos2::new(bounds.left(), rect.top()),
                            rect.left_bottom(),
                        ),
                        Rect::from_min_max(
                            rect.right_top(),
                            Pos2::new(bounds.right(), rect.bottom()),
                        ),
                    ] {
                        painter.rect_filled(outside, 0, shade);
                    }
                    painter.rect_stroke(
                        rect,
                        0,
                        Stroke::new(2., Color32::from_rgb(85, 232, 212)),
                        egui::StrokeKind::Inside,
                    );
                    for p in (mode != CaptureMode::Window && chosen.is_some())
                        .then_some([
                            rect.left_top(),
                            rect.right_top(),
                            rect.left_bottom(),
                            rect.right_bottom(),
                            rect.center_top(),
                            rect.center_bottom(),
                            rect.left_center(),
                            rect.right_center(),
                        ])
                        .into_iter()
                        .flatten()
                    {
                        painter.circle_filled(p, 4., Color32::WHITE);
                        painter.circle_stroke(p, 4., Stroke::new(1., appearance::ACCENT));
                    }
                    let label = if mode == CaptureMode::Window || chosen.is_none() {
                        selection
                            .windows
                            .iter()
                            .find(|w| Some(w.id) == selection.window_id)
                            .or(hovered_window)
                            .map_or("Window".into(), |w| w.name.clone())
                    } else {
                        selection::selection_to_pixels(
                            [rect.left() - bounds.left(), rect.top() - bounds.top()],
                            [rect.right() - bounds.left(), rect.bottom() - bounds.top()],
                            [bounds.width(), bounds.height()],
                            [selection.image.width(), selection.image.height()],
                        )
                        .map_or_else(String::new, |r| format!("{} × {} px", r.width, r.height))
                    };
                    let label_rect = Rect::from_min_size(
                        Pos2::new(rect.left().max(8.), (rect.top() - 31.).max(8.)),
                        Vec2::new(300_f32.min(bounds.width() - 16.), 26.),
                    );
                    painter.rect_filled(label_rect, 5, appearance::NAVY);
                    painter.with_clip_rect(label_rect.shrink(5.)).text(
                        label_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(12.),
                        Color32::WHITE,
                    );
                } else {
                    painter.rect_filled(bounds, 0, shade);
                }
                if let Some(p) = pointer.filter(|p| !bar.contains(*p))
                    && chosen.is_none()
                {
                    let stroke = Stroke::new(1., Color32::from_white_alpha(170));
                    painter.line_segment(
                        [
                            Pos2::new(bounds.left(), p.y),
                            Pos2::new(bounds.right(), p.y),
                        ],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            Pos2::new(p.x, bounds.top()),
                            Pos2::new(p.x, bounds.bottom()),
                        ],
                        stroke,
                    );
                }
                if selection.magnifier
                    && mode != CaptureMode::Window
                    && (chosen.is_none() || selection.drag.active)
                    && let Some(p) = pointer.filter(|p| !bar.contains(*p))
                {
                    selection
                        .magnifier_preview
                        .paint(painter, p, bounds, bar, &selection.image);
                }
                let hint = if mode == CaptureMode::Window {
                    "Click a window · Enter to capture · Esc to cancel"
                } else if mode == CaptureMode::Scrolling {
                    "Click a window or drag an area · Adjust edges · Start scrolling"
                } else {
                    "Drag to select · Move inside / resize edges · Enter to capture"
                };
                let hintrect = Rect::from_center_size(
                    Pos2::new(bounds.center().x, 36.),
                    Vec2::new(580_f32.min(bounds.width() - 16.), 30.),
                );
                painter.rect_filled(hintrect, 8, appearance::NAVY);
                painter.text(
                    hintrect.center(),
                    egui::Align2::CENTER_CENTER,
                    hint,
                    egui::FontId::proportional(13.),
                    Color32::WHITE,
                );
                if confirm {
                    if matches!(mode, CaptureMode::Window | CaptureMode::Scrolling) {
                        if let Some(w) = selection
                            .windows
                            .iter()
                            .find(|w| Some(w.id) == selection.window_id)
                        {
                            window = Some((
                                w.id,
                                if mode == CaptureMode::Scrolling {
                                    chosen.and_then(|r| window_selection(r, window_rect(w)))
                                } else {
                                    None
                                },
                            ));
                        }
                    } else if let Some(rect) = chosen {
                        accepted = selection::selection_to_pixels(
                            [rect.left() - bounds.left(), rect.top() - bounds.top()],
                            [rect.right() - bounds.left(), rect.bottom() - bounds.top()],
                            [bounds.width(), bounds.height()],
                            [selection.image.width(), selection.image.height()],
                        );
                    }
                }
            });
        if cancel {
            self.cancel_selection(ctx);
        } else if let Some((id, crop)) = window {
            self.capture_selected_window(ctx, id, crop);
        } else if let Some(rect) = accepted {
            self.accept_selection(ctx, rect);
        }
    }

    fn capture_selected_window(&mut self, ctx: &egui::Context, id: u32, crop: Option<[f32; 4]>) {
        let Some(selection) = self.selection.take() else {
            return;
        };
        self.pending_capture = Some(selection.return_hidden);
        self.generation += 1;
        let generation = self.generation;
        let pointer = self.settings.include_pointer;
        let demo = self.demo;
        let scrolling = self.settings.capture_mode == CaptureMode::Scrolling;
        let cancel = Arc::new(AtomicBool::new(false));
        self.capture_cancel = Some(cancel.clone());
        self.hidden.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            if cancel.load(Ordering::Acquire) {
                return;
            }
            let result = if demo {
                let w = selection.windows.iter().find(|w| w.id == id).unwrap();
                let sx = selection.image.width() as f64 / selection.display.width as f64;
                let sy = selection.image.height() as f64 / selection.display.height as f64;
                selection::crop_selection(
                    &selection.image,
                    selection::PixelRect {
                        x: ((w.x - selection.display.x) as f64 * sx).round() as u32,
                        y: ((w.y - selection.display.y) as f64 * sy).round() as u32,
                        width: (w.width as f64 * sx).round() as u32,
                        height: (w.height as f64 * sy).round() as u32,
                    },
                    CaptureShape::Rectangle,
                    0,
                )
            } else {
                capture::capture_window(id, pointer)
            };
            let result = result.map(|image| {
                if demo && scrolling {
                    image::RgbaImage::from_fn(image.width(), image.height(), demo_scroll_pixel)
                } else {
                    image
                }
            });
            let _ = tx.send(WorkEvent::WindowCaptured(
                generation,
                id,
                crop,
                Local::now(),
                result,
            ));
            ctx.request_repaint();
        });
    }

    pub(super) fn start_scroll_session(
        &mut self,
        ctx: &egui::Context,
        selection: Selection,
        rect: selection::PixelRect,
    ) {
        if rect.width < 32 || rect.height < 48 {
            self.error = Some("Select a scrolling area at least 32 × 48 pixels.".into());
            self.restore_window(ctx, false);
            return;
        }
        match selection::crop_selection(&selection.image, rect, CaptureShape::Rectangle, 0) {
            Ok(image) => {
                self.scroll_session=Some(ScrollSession {capture_size: selection.image.dimensions(), display:selection.display,rect,captured_at:selection.captured_at,
                    stitcher:Some(Stitcher::new(image)),message:"Scroll the target down, leaving at least a third overlapping. Then capture the next section.".into(),texture:None,placement_frames:0,demo_offset:0,window_id:None,window_crop:None,paused:false,finish_requested:false,next_capture:std::time::Instant::now()});
                self.restore_frames_left = 0;
                ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::ZERO));
                self.show(ctx);
            }
            Err(error) => {
                self.error = Some(error);
                self.restore_window(ctx, false);
            }
        }
    }

    pub(super) fn start_window_scroll(
        &mut self,
        ctx: &egui::Context,
        id: u32,
        crop: Option<[f32; 4]>,
        image: image::RgbaImage,
        at: DateTime<Local>,
    ) {
        let Some(display) = self
            .displays
            .iter()
            .find(|d| Some(d.id) == self.settings.display_id)
            .or(self.displays.first())
            .cloned()
        else {
            return;
        };
        let size = image.dimensions();
        let window_crop = crop.and_then(|[left, top, right, bottom]| {
            selection::selection_to_pixels([left, top], [right, bottom], [1., 1.], [size.0, size.1])
        });
        let rect = window_crop.unwrap_or(selection::PixelRect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        });
        if rect.width < 32 || rect.height < 48 {
            self.error = Some("Select a scrolling area at least 32 × 48 pixels.".into());
            self.restore_window(ctx, false);
            return;
        }
        let stitcher = if window_crop.is_some() {
            match selection::crop_selection(&image, rect, CaptureShape::Rectangle, 0) {
                Ok(area) => Stitcher::region(area),
                Err(error) => {
                    self.error = Some(error);
                    self.restore_window(ctx, false);
                    return;
                }
            }
        } else {
            Stitcher::window(image)
        };
        self.scroll_session = Some(ScrollSession {
            display,
            rect,
            capture_size: size,
            captured_at: at,
            stitcher: Some(stitcher),
            message: "Scroll down in the selected window. Sections are collected automatically."
                .into(),
            texture: None,
            placement_frames: 0,
            demo_offset: 0,
            window_id: Some(id),
            window_crop,
            paused: false,
            finish_requested: false,
            next_capture: std::time::Instant::now() + Duration::from_millis(700),
        });
        self.restore_frames_left = 0;
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::ZERO));
        self.show(ctx);
    }

    pub(super) fn capture_scroll_section(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(session) = &mut self.scroll_session else {
            return;
        };
        let Some(mut stitcher) = session.stitcher.take() else {
            return;
        };
        let display = session.display.clone();
        let rect = session.rect;
        let capture_size = session.capture_size;
        let window_id = session.window_id;
        let window_crop = session.window_crop;
        let final_sample = session.finish_requested;
        let pointer = self.settings.include_pointer;
        let demo = self.demo;
        session.demo_offset += 1;
        let demo_offset = session.demo_offset.div_ceil(2) * rect.height / 3;
        let generation = self.generation;
        self.busy = true;
        if window_id.is_none() {
            self.hidden.store(true, Ordering::Relaxed);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if window_id.is_none() {
                std::thread::sleep(Duration::from_millis(250));
            }
            let capture = || {
                if demo {
                    Ok(image::RgbaImage::from_fn(
                        capture_size.0,
                        capture_size.1,
                        |x, y| demo_scroll_pixel(x, y + demo_offset),
                    ))
                } else if let Some(id) = window_id {
                    capture::capture_window(id, pointer)
                } else {
                    capture::capture_display(display.id, pointer)
                }
            };
            let mut observe = || {
                capture().and_then(|image| {
                    if window_id.is_some() {
                        if image.dimensions() != capture_size {
                            return Err(
                                "The window changed size. Finish this capture and start another."
                                    .into(),
                            );
                        }
                        return match window_crop {
                            Some(rect) => {
                                selection::crop_selection(&image, rect, CaptureShape::Rectangle, 0)
                            }
                            None => Ok(image),
                        }
                        .and_then(|image| stitcher.observe_window(image));
                    }
                    if image.dimensions() != capture_size {
                        return Err(
                            "The display changed size. Finish this capture and start another."
                                .into(),
                        );
                    }
                    selection::crop_selection(&image, rect, CaptureShape::Rectangle, 0)
                        .and_then(|image| stitcher.append(image))
                })
            };
            let mut result = observe();
            if final_sample && window_id.is_some() && result.is_ok() {
                std::thread::sleep(Duration::from_millis(250));
                result = observe().map(|added| added || result.unwrap_or(false));
            }
            let _ = tx.send(WorkEvent::ScrollSection(
                generation,
                Box::new(stitcher),
                result,
                final_sample,
            ));
            ctx.request_repaint();
        });
    }
    pub(super) fn finish_scroll_capture(&mut self, ctx: &egui::Context) {
        let Some(session) = self.scroll_session.take() else {
            return;
        };
        self.generation += 1;
        self.restore_window(ctx, false);
        if let Some(stitcher) = session.stitcher {
            self.save_captured_image(ctx, stitcher.image, session.captured_at);
        }
    }

    pub(super) fn scroll_session_ui(&mut self, ctx: &egui::Context) {
        let Some(session) = &mut self.scroll_session else {
            return;
        };
        if session.placement_frames < 4 {
            let unit = Self::desktop_units_per_point(ctx);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(560., 320.)));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
                session.display.x as f32 / unit + 24.,
                session.display.y as f32 / unit + 48.,
            )));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            session.placement_frames += 1;
            ctx.request_repaint();
        }
        let mut next = false;
        let mut finish = false;
        let mut cancel = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(appearance::PAPER).inner_margin(18))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Capture Now").size(21.).strong());
                    ui.weak(if session.window_id.is_some() { "Automatic sections" } else { "Scrolling" });
                    if let Some(stitcher) = &session.stitcher {
                        ui.weak(format!(
                            "{} section{} · {} × {}",
                            stitcher.sections,
                            if stitcher.sections == 1 { "" } else { "s" },
                            stitcher.image.width(),
                            stitcher.image.height()
                        ));
                    }
                });
                ui.add_space(8.);
                ui.horizontal_top(|ui| {
                    if let Some(stitcher) = &session.stitcher
                        && session.texture.is_none() {
                        let thumbnail = image::imageops::thumbnail(&stitcher.image, 200, 240);
                        session.texture = Some(load_texture(ctx, "scroll-preview", &thumbnail));
                    }
                    if let Some(texture) = &session.texture {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::new(100., 120.)).maintain_aspect_ratio(true));
                    }
                    ui.vertical(|ui| {
                        ui.set_width(380.);
                        ui.label(&session.message);
                        ui.add_space(8.);
                        if session.window_id.is_some() {
                            ui.strong("1. Scroll the selected window");
                            ui.small("Scroll steadily, keeping some content overlapping. Pause or finish whenever you like.");
                            ui.strong("2. Finish when you have enough");
                            ui.small(if session.window_crop.is_some() { "Only your selected area is included." } else { "The scrolling content is collected automatically." });
                        } else {
                            ui.small("Scroll down, then capture the next section.");
                        }
                    });
                });
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    if session.window_id.is_some() {
                        if ui.add_enabled(!session.finish_requested, egui::Button::new(if session.paused { "Resume" } else { "Pause" })).clicked() {
                            session.paused = !session.paused;
                        }
                    } else {
                        next=ui.add_enabled(!self.busy,appearance::primary("Capture next section")).clicked();
                    }
                    finish = ui
                        .add_enabled(!session.finish_requested, appearance::primary("Finish & edit"))
                        .clicked();
                    cancel = ui
                        .add(egui::Button::new("Cancel"))
                        .clicked();
                });
            });
        if cancel {
            self.scroll_session = None;
            self.generation += 1;
            self.busy = false;
            self.restore_window(ctx, false);
            self.status = "Scrolling capture canceled".into();
            return;
        }
        if finish {
            if session.paused && !self.busy {
                self.finish_scroll_capture(ctx);
                return;
            }
            if session.window_id.is_some() || self.busy {
                session.finish_requested = true;
                session.paused = true;
                session.message = "Finishing the current section…".into();
                if !self.busy {
                    self.capture_scroll_section(ctx);
                }
            } else {
                self.finish_scroll_capture(ctx);
            }
            return;
        }
        if session.window_id.is_some() && !session.paused && !session.finish_requested {
            if !self.busy && std::time::Instant::now() >= session.next_capture {
                next = true;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if next {
            self.capture_scroll_section(ctx);
        }
    }
}

pub(super) fn demo_scroll_pixel(x: u32, y: u32) -> image::Rgba<u8> {
    let n = x
        .wrapping_mul(1973)
        .wrapping_add(y.wrapping_mul(9277))
        .wrapping_mul(26699);
    image::Rgba([
        (n % 251) as u8,
        ((n >> 8) % 251) as u8,
        ((n >> 16) % 251) as u8,
        255,
    ])
}

/// Ratios remain valid across logical/display pixels and native window capture scale.
fn window_selection(area: Rect, window: Rect) -> Option<[f32; 4]> {
    let area = area.intersect(window);
    if area.min.distance(window.min) < 1. && area.max.distance(window.max) < 1. {
        return None;
    }
    Some([
        (area.left() - window.left()) / window.width(),
        (area.top() - window.top()) / window.height(),
        (area.right() - window.left()) / window.width(),
        (area.bottom() - window.top()) / window.height(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_window_click_drag_and_adjust_share_the_same_boundary() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let window = Rect::from_min_max(egui::pos2(100., 80.), egui::pos2(900., 650.));
        let windows = [(7, window)];
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let gesture = |drag: &mut RegionDrag, selected: &mut Option<u32>, a, b| {
            drag.update_window(
                &[
                    button(a, true),
                    egui::Event::PointerMoved(b),
                    button(b, false),
                ],
                bounds,
                Rect::NOTHING,
                &windows,
                selected,
            )
        };
        let mut drag = RegionDrag::default();
        let mut selected = None;
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(200., 200.),
            egui::pos2(200., 200.),
        );
        assert_eq!(selected, Some(7));
        assert_eq!(drag.bounds(), Some(window));
        assert_eq!(window_selection(drag.bounds().unwrap(), window), None);
        drag = RegionDrag::default();
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(300., 222.5),
            egui::pos2(700., 507.5),
        );
        assert_eq!(
            window_selection(drag.bounds().unwrap(), window),
            Some([0.25, 0.25, 0.75, 0.75])
        );
        let pixels =
            selection::selection_to_pixels([0.25, 0.25], [0.75, 0.75], [1., 1.], [1600, 1140])
                .unwrap();
        assert_eq!(
            (pixels.x, pixels.y, pixels.width, pixels.height),
            (400, 285, 800, 570)
        );
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(700., 350.),
            egui::pos2(1200., 350.),
        );
        assert_eq!(drag.bounds().unwrap().right(), 900.);
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(500., 350.),
            egui::pos2(-200., -200.),
        );
        assert_eq!(drag.bounds().unwrap().min, window.min);
        assert_eq!(selected, Some(7));
    }
    #[test]
    fn capture_selection_can_move_resize_and_stay_within_display() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        let controls = Rect::from_min_size(Pos2::new(100., 520.), Vec2::new(600., 70.));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let mut drag = RegionDrag::default();
        let gesture = |a, b, drag: &mut RegionDrag| {
            drag.update(
                &[
                    button(a, true),
                    egui::Event::PointerMoved(b),
                    button(b, false),
                ],
                bounds,
                controls,
            )
        };
        gesture(Pos2::new(100., 100.), Pos2::new(300., 300.), &mut drag);
        gesture(Pos2::new(200., 200.), Pos2::new(250., 230.), &mut drag);
        assert_eq!(
            drag.bounds(),
            Some(Rect::from_min_max(
                Pos2::new(150., 130.),
                Pos2::new(350., 330.)
            ))
        );
        gesture(Pos2::new(350., 230.), Pos2::new(430., 230.), &mut drag);
        assert_eq!(drag.bounds().unwrap().right(), 430.);
        gesture(Pos2::new(150., 130.), Pos2::new(80., 60.), &mut drag);
        assert_eq!(drag.bounds().unwrap().min, Pos2::new(80., 60.));
        gesture(
            drag.bounds().unwrap().center(),
            Pos2::new(-500., -500.),
            &mut drag,
        );
        assert_eq!(drag.bounds().unwrap().min, Pos2::ZERO);
        let rect = drag.bounds().unwrap();
        gesture(
            controls.center(),
            controls.center() + Vec2::splat(20.),
            &mut drag,
        );
        assert_eq!(drag.bounds(), Some(rect));
        gesture(Pos2::new(600., 100.), Pos2::new(950., 400.), &mut drag);
        assert_eq!(drag.bounds().unwrap().right(), 800.);
    }
    #[test]
    fn region_drag_handles_coalesced_events_and_ignores_controller_clicks() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        let bar = Rect::from_min_size(Pos2::new(100., 500.), Vec2::new(600., 80.));
        let a = Pos2::new(90., 120.);
        let b = Pos2::new(600., 450.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        for timing in 0..3 {
            let mut drag = RegionDrag::default();
            let events = [
                button(a, true),
                egui::Event::PointerMoved(b),
                button(b, false),
            ];
            if timing == 0 {
                drag.update(&events, bounds, bar);
            } else {
                drag.update(&events[..1], bounds, bar);
                if timing == 1 {
                    drag.update(&events[1..], bounds, bar);
                } else {
                    for event in &events[1..] {
                        drag.update(std::slice::from_ref(event), bounds, bar);
                    }
                }
            }
            assert_eq!(drag.bounds(), Some(Rect::from_two_pos(a, b)));
            drag.update(
                &[
                    button(bar.center(), true),
                    button(bar.center(), false),
                    egui::Event::PointerMoved(Pos2::ZERO),
                ],
                bounds,
                bar,
            );
            assert_eq!(drag.bounds(), Some(Rect::from_two_pos(a, b)));
        }
    }
}
