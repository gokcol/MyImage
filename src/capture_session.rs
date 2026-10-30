use super::*;
use crate::{capture::CaptureMode, scroll_capture::Stitcher};

#[derive(Default)]
pub(super) struct RegionDrag {
    start: Option<Pos2>,
    end: Option<Pos2>,
    active: bool,
    completed: bool,
    adjustment: Option<(Pos2, Rect, [bool; 4])>,
}
impl RegionDrag {
    /// A click snaps to the frontmost window; a drag remains a free region.
    fn update_smart_region(
        &mut self,
        events: &[egui::Event],
        bounds: Rect,
        controls: Rect,
        windows: &[(u32, Rect)],
    ) {
        for event in events {
            let snap = if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } = event
            {
                (self.active
                    && self.adjustment.is_none()
                    && self.start.is_some_and(|p| p.distance(*pos) < 3.))
                .then(|| {
                    windows
                        .iter()
                        .find(|(_, r)| r.contains(*pos))
                        .map(|(_, r)| r.intersect(bounds))
                })
                .flatten()
            } else {
                None
            };
            self.update(std::slice::from_ref(event), bounds, controls);
            if let Some(rect) = snap {
                self.start = Some(rect.min);
                self.end = Some(rect.max);
                self.completed = true;
            }
        }
    }
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
                    self.completed = false;
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
                    self.completed = self.bounds().is_some();
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

#[derive(Default)]
pub(super) struct MultiRegions {
    rects: Vec<Rect>,
    selected: Option<usize>,
}
impl MultiRegions {
    fn promote(&mut self, drag: &RegionDrag) {
        if let Some(rect) = drag.bounds() {
            self.rects.push(rect);
            self.selected = Some(self.rects.len() - 1);
        }
    }
    fn remove_selected(&mut self, drag: &mut RegionDrag) {
        if let Some(index) = self.selected.take() {
            self.rects.remove(index);
        }
        *drag = RegionDrag::default();
    }
    fn preview(&self, drag: &RegionDrag) -> Vec<Rect> {
        let mut rects: Vec<_> = self
            .rects
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if Some(i) == self.selected {
                    drag.bounds().unwrap_or(*r)
                } else {
                    *r
                }
            })
            .collect();
        if self.selected.is_none()
            && let Some(rect) = drag.bounds()
        {
            rects.push(rect);
        }
        rects
    }
    fn update(
        &mut self,
        drag: &mut RegionDrag,
        events: &[egui::Event],
        bounds: Rect,
        controls: Rect,
        windows: &[(u32, Rect)],
    ) {
        for event in events {
            if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
                ..
            } = event
                && bounds.contains(*pos)
                && !controls.contains(*pos)
            {
                let hit = (!modifiers.command)
                    .then(|| self.rects.iter().rposition(|r| r.expand(7.).contains(*pos)))
                    .flatten();
                if hit.is_none() && self.rects.len() >= 32 {
                    continue;
                }
                self.selected = hit;
                *drag = RegionDrag::default();
                if let Some(index) = hit {
                    drag.start = Some(self.rects[index].min);
                    drag.end = Some(self.rects[index].max);
                }
            }
            drag.completed = false;
            drag.update_smart_region(std::slice::from_ref(event), bounds, controls, windows);
            if drag.completed
                && let Some(rect) = drag.bounds()
            {
                if let Some(index) = self.selected {
                    self.rects[index] = rect;
                } else if self.rects.len() < 32 {
                    self.rects.push(rect);
                    self.selected = Some(self.rects.len() - 1);
                }
            }
        }
    }
}

fn capture_on_completion(enabled: bool, mode: CaptureMode, completed: bool) -> bool {
    enabled
        && completed
        && matches!(
            mode,
            CaptureMode::Region | CaptureMode::Window | CaptureMode::Text
        )
}

pub(super) struct CapturedFrame {
    pub display: DisplayInfo,
    pub image: image::RgbaImage,
    pub windows: Vec<capture::WindowInfo>,
}
pub(super) struct SelectionSession {
    pub screens: Vec<Selection>,
    pub return_hidden: bool,
}

fn overlay_id(display: &DisplayInfo) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("capture-screen", display.id))
}

fn overlay_geometry(display: &DisplayInfo, desktop_units_per_point: f32) -> (Pos2, Vec2) {
    (
        Pos2::new(display.x as f32, display.y as f32) / desktop_units_per_point,
        Vec2::new(display.width as f32, display.height as f32) / desktop_units_per_point,
    )
}

impl SelectionSession {
    pub fn new(
        ctx: &egui::Context,
        frames: Vec<CapturedFrame>,
        captured_at: DateTime<Local>,
        mode: CaptureMode,
        return_hidden: bool,
    ) -> Self {
        Self {
            return_hidden,
            screens: frames
                .into_iter()
                .map(|frame| Selection {
                    texture: load_texture(ctx, "selection", &frame.image),
                    display: frame.display,
                    image: frame.image,
                    windows: frame.windows,
                    captured_at,
                    mode,
                    return_hidden,
                    placement_frames: 0,
                    drag: Default::default(),
                    multi: Default::default(),
                    window_id: None,
                    hover_cycle: 0,
                    magnifier: true,
                    magnifier_preview: Default::default(),
                    pointer_ready: false,
                })
                .collect(),
        }
    }

    pub fn close(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::Visible(false));
        for screen in self.screens.iter().skip(1) {
            ctx.send_viewport_cmd_to(overlay_id(&screen.display), ViewportCommand::Visible(false));
            ctx.send_viewport_cmd_to(overlay_id(&screen.display), ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

#[derive(Debug, PartialEq)]
enum SelectionAction {
    Cancel,
    Window(u32, Option<[f32; 4]>),
    Regions(Vec<selection::PixelRect>),
    Region(selection::PixelRect),
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
    pub single_frame_warning: bool,
    pub next_capture: std::time::Instant,
    placement_frames: u8,
    demo_offset: u32,
}

impl MyImageApp {
    pub(super) fn selection_ui(&mut self, ctx: &egui::Context) {
        let Some(mut session) = self.selection.take() else {
            return;
        };
        let mut chosen = None;
        for (index, screen) in session.screens.iter_mut().enumerate() {
            // Mode changes in one controller apply to every screen.
            if screen.mode != self.settings.capture_mode {
                screen.mode = self.settings.capture_mode;
                screen.drag = RegionDrag::default();
                screen.multi = MultiRegions::default();
                screen.window_id = None;
                screen.hover_cycle = 0;
            }
            if index == 0 {
                // Immediate viewports repaint through their parent. Keep the
                // root visible as one overlay so native child input can wake
                // it reliably, including mouse release, Enter and Escape.
                if let Some(action) = self.selection_overlay_ui(ctx, screen) {
                    chosen = Some((index, action));
                }
                crate::pointer_style::paint(ctx, self.settings.pointer_style);
            } else {
                let divisor = ctx.zoom_factor()
                    * if cfg!(target_os = "windows") {
                        screen.display.scale_factor
                    } else {
                        1.
                    };
                let (position, size) = overlay_geometry(&screen.display, divisor);
                let builder = egui::ViewportBuilder::default()
                    .with_title(format!("MyImage Capture — {}", screen.display.name))
                    .with_position(position)
                    .with_inner_size(size)
                    .with_min_inner_size(Vec2::ZERO)
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_taskbar(false)
                    .with_has_shadow(false)
                    .with_window_level(WindowLevel::AlwaysOnTop)
                    .with_active(false)
                    .with_visible(screen.placement_frames >= 4);
                ctx.show_viewport_immediate(overlay_id(&screen.display), builder, |child, _| {
                    if let Some(action) = self.selection_overlay_ui(child, screen) {
                        chosen = Some((index, action));
                    }
                    crate::pointer_style::paint(child, self.settings.pointer_style);
                });
            }
            if chosen.is_some() {
                break;
            }
        }
        if let Some((index, action)) = chosen {
            session.close(ctx);
            let screen = session.screens.remove(index);
            match action {
                SelectionAction::Cancel => {
                    self.restore_window(ctx, session.return_hidden);
                    self.status = "Capture canceled".into();
                }
                SelectionAction::Window(id, crop) => {
                    self.capture_selected_window(ctx, screen, id, crop)
                }
                SelectionAction::Regions(regions) => {
                    self.queue_capture_output(ctx, screen.captured_at, move || {
                        selection::crop_regions(&screen.image, &regions)
                    })
                }
                SelectionAction::Region(rect) => self.accept_selection(ctx, screen, rect),
            }
        } else {
            self.selection = Some(session);
        }
    }

    fn selection_overlay_ui(
        &mut self,
        ctx: &egui::Context,
        selection: &mut Selection,
    ) -> Option<SelectionAction> {
        let menu_was_open = egui::Popup::is_any_open(ctx);
        if ctx.input(|i| i.viewport().close_requested())
            || (!menu_was_open && ctx.input(|i| i.key_pressed(egui::Key::Escape)))
        {
            return Some(SelectionAction::Cancel);
        }
        let placing_overlay = selection.placement_frames < 4;
        if placing_overlay {
            if selection.placement_frames == 0 && ctx.viewport_id() == egui::ViewportId::ROOT {
                if cfg!(target_os = "windows") {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
                }
                ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::ZERO));
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            }
            let divisor = if cfg!(target_os = "windows") {
                ctx.pixels_per_point()
            } else {
                ctx.zoom_factor()
            };
            let (position, size) = overlay_geometry(&selection.display, divisor);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
            selection.placement_frames += 1;
            ctx.request_repaint();
        }
        // Placement changes invalidate window-relative coordinates. Only real
        // events on this overlay enable selection; leaving clears stale hover.
        let pointer_moved = ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }
                )
            })
        });
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerGone))
        }) {
            selection.pointer_ready = false;
            selection.drag.active = false;
            selection.drag.adjustment = None;
        } else if !placing_overlay && pointer_moved {
            selection.pointer_ready = true;
            if !ctx.input(|i| i.viewport().focused.unwrap_or(i.focused)) {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
        }
        let mut mode = selection.mode;
        let old_mode = mode;
        if !menu_was_open
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
            && matches!(mode, CaptureMode::Region | CaptureMode::Window)
        {
            mode = if mode == CaptureMode::Window {
                CaptureMode::Region
            } else {
                CaptureMode::Window
            };
            selection.drag = RegionDrag::default();
            selection.window_id = None;
        }
        selection.drag.completed = false;
        let mut destination_changed = false;
        let mut accepted = None;
        let mut cancel = false;
        let mut accepted_regions = Vec::new();
        let mut window = None;
        // Place the controller before the canvas so its hit region takes priority.
        let area = egui::Area::new(egui::Id::new((
            "capture-now-controls",
            selection.display.id,
        )))
        .order(egui::Order::Foreground);
        let area = if let Some(rect) = selection.drag.bounds().filter(|_| !selection.drag.active) {
            area.fixed_pos(controls_position(
                ctx.content_rect(),
                rect,
                Vec2::new(
                    if mode == CaptureMode::MultiRegion {
                        930.
                    } else {
                        770.
                    },
                    62.,
                ),
            ))
        } else {
            area.anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0., -24.))
        };
        let bar = area
            .show(ctx, |ui| {
                ui.push_id(selection.display.id, |ui| {
                    egui::Frame::new()
                        .fill(appearance::palette(ui.ctx()).navy)
                        .corner_radius(12)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 10.;
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Capture Now")
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                egui::ComboBox::from_id_salt("capture_mode_live")
                                    .selected_text(mode.label())
                                    .width(110.)
                                    .show_ui(ui, |ui| {
                                        for choice in [
                                            CaptureMode::Region,
                                            CaptureMode::MultiRegion,
                                            CaptureMode::Window,
                                            CaptureMode::FullScreen,
                                            CaptureMode::Scrolling,
                                            CaptureMode::Text,
                                        ] {
                                            ui.selectable_value(&mut mode, choice, choice.label());
                                        }
                                    })
                                    .response
                                    .on_hover_text(
                                        "Choose a capture type. Space switches Region / Window.",
                                    );
                                if mode == CaptureMode::Text {
                                    ui.label(
                                        egui::RichText::new("Text → clipboard")
                                            .color(Color32::WHITE),
                                    );
                                } else {
                                    destination_changed = crate::capture_panel::destination_picker(
                                        ui,
                                        &mut self.settings.capture_destination,
                                    );
                                }
                                ui.checkbox(
                                    &mut selection.magnifier,
                                    egui::RichText::new("Magnifier").color(Color32::WHITE),
                                );
                                if ui.button("Reset").clicked() {
                                    selection.drag = RegionDrag::default();
                                    selection.multi = MultiRegions::default();
                                    selection.window_id = None;
                                }
                                if ui.button("Cancel").clicked() {
                                    cancel = true;
                                }
                                if mode == CaptureMode::MultiRegion {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{}/32",
                                            selection.multi.rects.len()
                                        ))
                                        .color(Color32::WHITE),
                                    );
                                    if ui
                                        .add_enabled(
                                            selection.multi.selected.is_some(),
                                            egui::Button::new("Remove"),
                                        )
                                        .on_hover_text("Remove selected region · Delete")
                                        .clicked()
                                    {
                                        selection.multi.remove_selected(&mut selection.drag);
                                    }
                                }
                                let ready = mode == CaptureMode::FullScreen
                                    || selection.drag.bounds().is_some()
                                    || (mode == CaptureMode::MultiRegion
                                        && !selection.multi.rects.is_empty());
                                if ui
                                    .add_enabled(
                                        ready,
                                        appearance::primary(
                                            ui.ctx(),
                                            if mode == CaptureMode::Scrolling {
                                                "Start scrolling"
                                            } else if mode == CaptureMode::Text {
                                                "Copy text"
                                            } else {
                                                self.settings.capture_destination.capture_label()
                                            },
                                        ),
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
                });
            })
            .response
            .rect;
        if mode != old_mode {
            selection.drag = RegionDrag::default();
            selection.multi = MultiRegions::default();
            selection.mode = mode;
            selection.window_id = None;
            selection.hover_cycle = 0;
            self.settings.capture_mode = mode;
        }
        let menu_interaction = menu_was_open || egui::Popup::is_any_open(ctx);
        let mut confirm = accepted.take().is_some()
            || (!menu_interaction && ctx.input(|i| i.key_pressed(egui::Key::Enter)));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let bounds = ui.max_rect();
                let response = ui.allocate_rect(bounds, egui::Sense::click_and_drag());
                let pointer = ctx.input(|i| i.pointer.hover_pos()).filter(|p| {
                    !menu_interaction && selection.pointer_ready && bounds.contains(*p)
                });
                if response.hovered() && mode != CaptureMode::Window {
                    ctx.set_cursor_icon(
                        pointer.map_or(egui::CursorIcon::Crosshair, |p| {
                            if mode == CaptureMode::MultiRegion && let Some(rect) = selection.multi.rects.iter().rev().find(|r| r.expand(7.).contains(p)) {
                                RegionDrag { start: Some(rect.min), end: Some(rect.max), ..Default::default() }.cursor(p)
                            } else { selection.drag.cursor(p) }
                        }),
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
                if ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::PointerMoved(_)))
                }) {
                    selection.hover_cycle = 0;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                    selection.hover_cycle = selection.hover_cycle.wrapping_add(1);
                }
                let candidates: Vec<_> = pointer
                    .filter(|p| !bar.contains(*p))
                    .map(|p| {
                        selection
                            .windows
                            .iter()
                            .filter(|w| window_rect(w).contains(p))
                            .collect()
                    })
                    .unwrap_or_default();
                let hovered_window = (!candidates.is_empty())
                    .then(|| candidates[selection.hover_cycle % candidates.len()]);
                if mode == CaptureMode::Window && response.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if !placing_overlay && !menu_interaction && selection.pointer_ready && mode == CaptureMode::Region
                    && ctx.input(|i| i.events.iter().any(|e| matches!(e,
                        egui::Event::PointerButton {pos, button: egui::PointerButton::Primary, pressed: true, modifiers, ..}
                            if modifiers.command && bounds.contains(*pos) && !bar.contains(*pos))))
                {
                    selection.multi.promote(&selection.drag);
                    mode = CaptureMode::MultiRegion;
                    selection.mode = mode;
                }
                if mode == CaptureMode::MultiRegion && !menu_interaction && !placing_overlay {
                    if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                        selection.multi.remove_selected(&mut selection.drag);
                    }
                    if selection.pointer_ready {
                        let windows: Vec<_> = selection.windows.iter().map(|w| (w.id, window_rect(w))).collect();
                        selection.multi.update(&mut selection.drag, &ctx.input(|i| i.events.clone()), bounds, bar, &windows);
                    }
                } else if mode == CaptureMode::Window {
                    if !placing_overlay && selection.pointer_ready && !menu_interaction
                        && response.clicked()
                        && let Some(w) = hovered_window
                    {
                        let rect = window_rect(w).intersect(bounds);
                        selection.drag.start = Some(rect.min);
                        selection.drag.end = Some(rect.max);
                        selection.window_id = Some(w.id);
                        selection.drag.completed = true;
                    }
                } else if mode == CaptureMode::FullScreen {
                    confirm |= !placing_overlay && !menu_interaction && selection.pointer_ready && response.clicked()
                        && pointer.is_some_and(|p| !bar.contains(p));
                } else if !placing_overlay && selection.pointer_ready && !menu_interaction && mode == CaptureMode::Scrolling {
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
                } else if !placing_overlay && selection.pointer_ready && !menu_interaction && mode == CaptureMode::Text {
                    // Text capture is a free rectangle. A click or modifier must
                    // not silently snap to a window or become an image capture.
                    selection.drag.update(
                        &ctx.input(|i| i.events.clone()),
                        bounds,
                        bar,
                    );
                } else if !placing_overlay && selection.pointer_ready && !menu_interaction && mode == CaptureMode::Region {
                    let windows: Vec<_> = hovered_window
                        .into_iter()
                        .chain(
                            selection
                                .windows
                                .iter()
                                .filter(|w| Some(w.id) != hovered_window.map(|w| w.id)),
                        )
                        .map(|w| (w.id, window_rect(w)))
                        .collect();
                    selection.drag.update_smart_region(
                        &ctx.input(|i| i.events.clone()),
                        bounds,
                        bar,
                        &windows,
                    );
                }
                confirm |= !placing_overlay
                    && !menu_interaction
                    && capture_on_completion(self.settings.capture_on_release, mode, selection.drag.completed);
                let chosen = selection.drag.bounds().map(|r| r.intersect(bounds));
                let selected = (mode == CaptureMode::FullScreen).then_some(bounds).or(chosen).or_else(|| {
                    (mode != CaptureMode::Text && !selection.drag.active)
                        .then(|| hovered_window.map(|w| window_rect(w).intersect(bounds)))
                        .flatten()
                });
                let painter = ui.painter();
                painter.image(
                    selection.texture.id(),
                    bounds,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                    Color32::WHITE,
                );
                let shade = Color32::from_black_alpha(145);
                if mode == CaptureMode::MultiRegion {
                    painter.rect_filled(bounds, 0, shade);
                    for (index, rect) in selection.multi.preview(&selection.drag).into_iter().enumerate() {
                        let uv = Rect::from_min_max(
                            ((rect.min - bounds.min) / bounds.size()).to_pos2(),
                            ((rect.max - bounds.min) / bounds.size()).to_pos2());
                        painter.image(selection.texture.id(), rect, uv, Color32::WHITE);
                        painter.rect_stroke(rect, 0, Stroke::new(2., appearance::palette(ui.ctx()).accent), egui::StrokeKind::Inside);
                        let badge = Rect::from_min_size(rect.min + Vec2::splat(4.), Vec2::new(25.,22.));
                        painter.rect_filled(badge, 4, appearance::palette(ui.ctx()).navy);
                        painter.text(badge.center(), egui::Align2::CENTER_CENTER, (index+1).to_string(), egui::FontId::proportional(12.), Color32::WHITE);
                    }
                    if let Some(rect) = chosen {
                        for p in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom(), rect.center_top(), rect.center_bottom(), rect.left_center(), rect.right_center()] {
                            painter.circle_filled(p, 4., Color32::WHITE);
                            painter.circle_stroke(p, 4., Stroke::new(1., appearance::palette(ui.ctx()).accent));
                        }
                    }
                } else if let Some(rect) = selected {
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
                        painter.circle_stroke(p, 4., Stroke::new(1., appearance::palette(ui.ctx()).accent));
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
                    painter.rect_filled(label_rect, 5, appearance::palette(ui.ctx()).navy);
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
                    && !matches!(mode, CaptureMode::Window | CaptureMode::FullScreen)
                    && (chosen.is_none() || selection.drag.active)
                    && let Some(p) = pointer.filter(|p| !bar.contains(*p))
                {
                    selection
                        .magnifier_preview
                        .paint(painter, p, bounds, bar, &selection.image);
                }
                let hint = if mode == CaptureMode::FullScreen {
                    "Click this screen to capture it · Move to any other screen to choose it · Esc cancels"
                } else if mode == CaptureMode::MultiRegion {
                    "Drag to add regions · Drag a region to adjust · Delete removes · Enter captures"
                } else if mode == CaptureMode::Text && self.settings.capture_on_release {
                    "Drag and release to copy text · Esc cancels"
                } else if mode == CaptureMode::Text {
                    "Drag around text · Adjust edges · Enter copies text · Esc cancels"
                } else if self.settings.capture_on_release && mode == CaptureMode::Window {
                    "Click a window to capture · Tab cycles overlapping windows · Esc cancels"
                } else if self.settings.capture_on_release && mode == CaptureMode::Region {
                    if cfg!(target_os = "macos") { "Drag and release to capture · ⌘-drag adds regions · Esc cancels" } else { "Drag and release to capture · Ctrl-drag adds regions · Esc cancels" }
                } else if mode == CaptureMode::Window {
                    "Click a window · Tab cycles overlapping windows · Enter captures"
                } else if mode == CaptureMode::Scrolling {
                    "Click a window or drag an area · Adjust edges · Start scrolling"
                } else if cfg!(target_os = "macos") {
                    "Click a window or drag an area · ⌘-drag adds regions · Enter captures"
                } else {
                    "Click a window or drag an area · Ctrl-drag adds regions · Enter captures"
                };
                let hintrect = Rect::from_center_size(
                    Pos2::new(bounds.center().x, 36.),
                    Vec2::new(700_f32.min(bounds.width() - 16.), 30.),
                );
                painter.rect_filled(hintrect, 8, appearance::palette(ui.ctx()).navy);
                painter.text(
                    hintrect.center(),
                    egui::Align2::CENTER_CENTER,
                    hint,
                    egui::FontId::proportional(13.),
                    Color32::WHITE,
                );
                if confirm && !placing_overlay && !selection.drag.active {
                    if mode == CaptureMode::FullScreen {
                        accepted = Some(selection::PixelRect { x: 0, y: 0,
                            width: selection.image.width(), height: selection.image.height() });
                    } else if mode == CaptureMode::MultiRegion {
                        accepted_regions = selection.multi.rects.iter().filter_map(|rect|
                            selection::selection_to_pixels(
                                [rect.left()-bounds.left(), rect.top()-bounds.top()],
                                [rect.right()-bounds.left(), rect.bottom()-bounds.top()],
                                [bounds.width(), bounds.height()],
                                [selection.image.width(), selection.image.height()])).collect();
                    } else if matches!(mode, CaptureMode::Window | CaptureMode::Scrolling) {
                        if let Some(w) = selection
                            .windows
                            .iter()
                            .find(|w| Some(w.id) == selection.window_id)
                            .or_else(|| {
                                (mode == CaptureMode::Window)
                                    .then_some(hovered_window)
                                    .flatten()
                            })
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
        if mode != old_mode || destination_changed {
            self.persist_settings();
        }
        if cancel {
            Some(SelectionAction::Cancel)
        } else if let Some((id, crop)) = window {
            Some(SelectionAction::Window(id, crop))
        } else if !accepted_regions.is_empty() {
            Some(SelectionAction::Regions(accepted_regions))
        } else {
            accepted.map(SelectionAction::Region)
        }
    }

    fn capture_selected_window(
        &mut self,
        ctx: &egui::Context,
        selection: Selection,
        id: u32,
        crop: Option<[f32; 4]>,
    ) {
        self.pending_capture = Some(selection.return_hidden);
        self.generation += 1;
        let generation = self.generation;
        let scrolling = self.settings.capture_mode == CaptureMode::Scrolling;
        // A moving pointer is not document content and can spoil an overlap.
        let pointer = self.settings.include_pointer && !scrolling;
        let demo = self.demo;
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
                selection.display,
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
                    stitcher:Some(Stitcher::new(image)),message:"Scroll the target down, leaving at least a third overlapping. Then capture the next section.".into(),texture:None,placement_frames:0,demo_offset:0,window_id:None,window_crop:None,paused:false,finish_requested:false,single_frame_warning:false,next_capture:std::time::Instant::now()});
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
        display: DisplayInfo,
        id: u32,
        crop: Option<[f32; 4]>,
        image: image::RgbaImage,
        at: DateTime<Local>,
    ) {
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
            single_frame_warning: false,
            next_capture: std::time::Instant::now() + Duration::from_millis(100),
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
                    capture::capture_window(id, false)
                } else {
                    capture::capture_display(display.id, false)
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
        let Some(session) = &mut self.scroll_session else {
            return;
        };
        let Some(stitcher) = &session.stitcher else {
            return;
        };
        if stitcher.sections == 1 && !session.single_frame_warning {
            session.single_frame_warning = true;
            session.finish_requested = false;
            session.paused = true;
            session.message = "No additional sections were captured. For a PDF, use continuous scrolling and move down in small steps. Resume to try again, or keep only the visible area.".into();
            return;
        }
        let Some(session) = self.scroll_session.take() else {
            return;
        };
        self.generation += 1;
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
            .frame(egui::Frame::new().fill(appearance::palette(ctx).paper).inner_margin(18))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Capture Now").size(21.).strong());
                    ui.weak("Scrolling");
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
                        if session.single_frame_warning {
                            // The recovery message above replaces the normal instructions.
                        } else if session.window_id.is_some() {
                            ui.strong("1. Scroll the selected window");
                            ui.small("Use small steps, keeping at least half the previous content visible. For PDFs, choose continuous scrolling in the PDF viewer.");
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
                            if !session.paused {
                                session.single_frame_warning = false;
                                session.message = "Scroll down in the selected window. Sections are collected automatically.".into();
                            }
                        }
                    } else {
                        next=ui.add_enabled(!self.busy,appearance::primary(ui.ctx(), "Capture next section")).clicked();
                    }
                    finish = ui
                        .add_enabled(!session.finish_requested, appearance::primary(ui.ctx(), if session.single_frame_warning { "Keep visible area" } else { self.settings.capture_destination.finish_label() }))
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

/// Keep confirmation controls close to the selection and inside the display.
fn controls_position(bounds: Rect, selected: Rect, size: Vec2) -> Pos2 {
    let x = (selected.center().x - size.x / 2.).clamp(
        bounds.left() + 8.,
        (bounds.right() - size.x - 8.).max(bounds.left() + 8.),
    );
    let below = selected.bottom() + 12.;
    let y = if below + size.y <= bounds.bottom() - 8. {
        below
    } else if selected.top() - 12. - size.y >= bounds.top() + 8. {
        selected.top() - 12. - size.y
    } else {
        bounds.bottom() - size.y - 12.
    };
    Pos2::new(x, y.max(bounds.top() + 8.))
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
    fn test_screens(ctx: &egui::Context, mode: CaptureMode) -> SelectionSession {
        let frames = [
            (1, 0, 0, 1., [240, 10, 20, 255]),
            (2, -800, -600, 2., [10, 20, 240, 255]),
        ]
        .into_iter()
        .map(|(id, x, y, scale, color)| CapturedFrame {
            display: DisplayInfo {
                id,
                name: format!("Display {id}"),
                x,
                y,
                width: 800,
                height: 600,
                scale_factor: scale,
                is_primary: id == 1,
            },
            image: image::RgbaImage::from_pixel(
                (800. * scale) as u32,
                (600. * scale) as u32,
                image::Rgba(color),
            ),
            windows: Vec::new(),
        })
        .collect();
        let mut session = SelectionSession::new(ctx, frames, Local::now(), mode, false);
        for screen in &mut session.screens {
            screen.placement_frames = 5;
            screen.magnifier = false;
        }
        session
    }

    fn overlay_frame(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        screen: &mut Selection,
        events: Vec<egui::Event>,
    ) -> Option<SelectionAction> {
        let mut action = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                events,
                ..Default::default()
            },
            |ctx| {
                action = app.selection_overlay_ui(ctx, screen);
            },
        );
        action
    }

    fn pointer_button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn either_display_selects_its_own_pixels_at_native_resolution() {
        for index in 0..2 {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let mut session = test_screens(&ctx, CaptureMode::Region);
            let screen = &mut session.screens[index];
            let start = Pos2::new(100., 150.);
            let end = Pos2::new(300., 270.);
            overlay_frame(&mut app, &ctx, screen, vec![]);
            overlay_frame(
                &mut app,
                &ctx,
                screen,
                vec![
                    egui::Event::PointerMoved(start),
                    pointer_button(start, true),
                ],
            );
            overlay_frame(&mut app, &ctx, screen, vec![egui::Event::PointerMoved(end)]);
            assert_eq!(
                overlay_frame(&mut app, &ctx, screen, vec![pointer_button(end, false)]),
                None
            );
            let action = overlay_frame(&mut app, &ctx, screen, vec![key(egui::Key::Enter)]);
            let Some(SelectionAction::Region(rect)) = action else {
                panic!("Expected region: {action:?}");
            };
            let scale = (index + 1) as u32;
            assert_eq!(
                (rect.x, rect.y, rect.width, rect.height),
                (100 * scale, 150 * scale, 200 * scale, 120 * scale)
            );
            let cropped =
                selection::crop_selection(&screen.image, rect, CaptureShape::Rectangle, 0).unwrap();
            assert_eq!(
                cropped.get_pixel(0, 0).0,
                if index == 0 {
                    [240, 10, 20, 255]
                } else {
                    [10, 20, 240, 255]
                }
            );
            assert!(session.screens[1 - index].drag.bounds().is_none());
        }
    }

    #[test]
    fn full_screen_click_chooses_either_screen_and_its_native_size() {
        for index in 0..2 {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let mut session = test_screens(&ctx, CaptureMode::FullScreen);
            let screen = &mut session.screens[index];
            overlay_frame(&mut app, &ctx, screen, vec![]);
            overlay_frame(&mut app, &ctx, screen, vec![]);
            let pos = Pos2::new(400., 300.);
            assert_eq!(
                overlay_frame(
                    &mut app,
                    &ctx,
                    screen,
                    vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)]
                ),
                None
            );
            let action = overlay_frame(&mut app, &ctx, screen, vec![pointer_button(pos, false)]);
            assert_eq!(
                action,
                Some(SelectionAction::Region(selection::PixelRect {
                    x: 0,
                    y: 0,
                    width: screen.image.width(),
                    height: screen.image.height(),
                }))
            );
        }
    }

    #[test]
    fn overlay_geometry_preserves_negative_origins_and_mixed_scaling() {
        let ctx = egui::Context::default();
        let session = test_screens(&ctx, CaptureMode::Region);
        let display = &session.screens[1].display;
        // macOS uses Quartz points; Windows uses physical pixels / per-screen DPI.
        for divisor in [1., 1.25, 2., 2.3] {
            let (position, size) = overlay_geometry(display, divisor);
            assert!((position * divisor - Pos2::new(-800., -600.)).length() < 0.001);
            assert!((size * divisor - Vec2::new(800., 600.)).length() < 0.001);
        }
    }

    #[test]
    fn leaving_overlay_clears_hover_and_unfinished_drag() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let mut session = test_screens(&ctx, CaptureMode::Region);
        let screen = &mut session.screens[0];
        overlay_frame(&mut app, &ctx, screen, vec![]);
        let pos = Pos2::new(100., 150.);
        overlay_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)],
        );
        assert!(screen.drag.active && screen.pointer_ready);
        overlay_frame(&mut app, &ctx, screen, vec![egui::Event::PointerGone]);
        assert!(!screen.drag.active && !screen.pointer_ready);
        assert_eq!(
            overlay_frame(&mut app, &ctx, screen, vec![key(egui::Key::Escape)]),
            Some(SelectionAction::Cancel)
        );
    }

    #[test]
    fn cancel_on_secondary_closes_all_overlays_and_restores_only_editor() {
        use std::{cell::RefCell, rc::Rc};
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let session = test_screens(&ctx, CaptureMode::Region);
        let ids: Vec<_> = session
            .screens
            .iter()
            .map(|s| overlay_id(&s.display))
            .collect();
        let secondary = ids[1];
        let shown = Rc::new(RefCell::new(Vec::new()));
        let shown_for_renderer = shown.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            shown_for_renderer.borrow_mut().push(viewport.ids.this);
            let _ = ctx.run(
                egui::RawInput {
                    viewport_id: viewport.ids.this,
                    viewports: [(viewport.ids.this, egui::ViewportInfo::default())]
                        .into_iter()
                        .collect(),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events: if viewport.ids.this == secondary {
                        vec![key(egui::Key::Escape)]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ctx| (viewport.viewport_ui_cb)(ctx),
            );
        });
        app.selection = Some(session);
        let output = ctx.run(egui::RawInput::default(), |ctx| app.selection_ui(ctx));
        assert_eq!(*shown.borrow(), vec![secondary]);
        assert!(app.selection.is_none());
        assert_eq!(app.status, "Capture canceled");
        for id in ids.into_iter().skip(1) {
            let commands = &output.viewport_output[&id].commands;
            assert!(commands.contains(&ViewportCommand::Visible(false)));
            assert!(commands.contains(&ViewportCommand::Close));
            assert!(!commands.contains(&ViewportCommand::Decorations(true)));
        }
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Decorations(true))
        );
        ctx.set_embed_viewports(true);
    }

    #[test]
    fn finish_without_scroll_keeps_session_open_and_warns_instead_of_saving() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.displays.push(DisplayInfo {
            id: 1,
            name: "Test display".into(),
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            scale_factor: 1.,
            is_primary: true,
        });
        app.start_window_scroll(
            &ctx,
            app.displays[0].clone(),
            42,
            Some([0., 0., 1., 1.]),
            image::RgbaImage::new(300, 400),
            Local::now(),
        );
        app.scroll_session.as_mut().unwrap().finish_requested = true;
        app.finish_scroll_capture(&ctx);
        let session = app
            .scroll_session
            .as_ref()
            .expect("single frame must not silently save");
        assert!(session.single_frame_warning);
        assert!(session.paused);
        assert!(!session.finish_requested);
        assert_eq!(session.stitcher.as_ref().unwrap().sections, 1);
        assert!(app.document.is_none());
        assert!(!app.busy);
        assert!(session.message.contains("No additional sections"));
    }
    #[test]
    fn fast_capture_only_finishes_completed_single_selections_when_enabled() {
        for mode in CaptureMode::ALL {
            assert!(!capture_on_completion(false, mode, true));
            assert!(!capture_on_completion(true, mode, false));
            assert_eq!(
                capture_on_completion(true, mode, true),
                matches!(
                    mode,
                    CaptureMode::Region | CaptureMode::Window | CaptureMode::Text
                )
            );
        }
    }

    #[test]
    fn text_capture_stays_a_free_region_with_space_and_command_drag() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.capture_mode = CaptureMode::Text;
        let image = image::RgbaImage::new(800, 600);
        let mut screen = Selection {
            display: DisplayInfo {
                id: 1,
                name: "Test display".into(),
                x: 0,
                y: 0,
                width: 800,
                height: 600,
                scale_factor: 1.,
                is_primary: true,
            },
            texture: load_texture(&ctx, "text-capture-test", &image),
            image,
            captured_at: Local::now(),
            drag: Default::default(),
            multi: Default::default(),
            mode: CaptureMode::Text,
            return_hidden: true,
            placement_frames: 4,
            windows: vec![capture::WindowInfo {
                id: 9,
                name: "Underlying window".into(),
                x: 50,
                y: 50,
                width: 600,
                height: 400,
            }],
            window_id: None,
            hover_cycle: 0,
            magnifier: false,
            magnifier_preview: Default::default(),
            pointer_ready: true,
        };
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.selection_overlay_ui(ctx, &mut screen);
                },
            )
        };
        frame(vec![]);
        let output = frame(vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"Text → clipboard"));
        assert!(labels.contains(&"Copy text"));
        let start = egui::pos2(100., 100.);
        let end = egui::pos2(300., 220.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        frame(vec![egui::Event::PointerMoved(end)]);
        frame(vec![button(end, false)]);
        let selection = &screen;
        assert_eq!(selection.mode, CaptureMode::Text);
        assert_eq!(
            selection.drag.bounds(),
            Some(Rect::from_min_max(start, end))
        );
        assert!(selection.multi.rects.is_empty());
        assert_eq!(app.settings.capture_mode, CaptureMode::Text);
        assert_eq!(
            app.settings.capture_destination,
            capture::CaptureDestination::Editor
        );
        assert!(app.document.is_none());
    }

    #[test]
    fn multiple_regions_add_adjust_remove_and_keep_preview_order() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(800.));
        let mut multi = MultiRegions::default();
        let mut drag = RegionDrag::default();
        let gesture =
            |multi: &mut MultiRegions, drag: &mut RegionDrag, a: Pos2, b: Pos2, command: bool| {
                let button = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers {
                        command,
                        ..Default::default()
                    },
                };
                multi.update(
                    drag,
                    &[
                        button(a, true),
                        egui::Event::PointerMoved(b),
                        button(b, false),
                    ],
                    bounds,
                    Rect::NOTHING,
                    &[],
                );
            };
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(50., 50.),
            egui::pos2(150., 150.),
            false,
        );
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(400., 400.),
            egui::pos2(300., 300.),
            false,
        );
        assert_eq!(multi.rects.len(), 2);
        assert_eq!(multi.rects[1].size(), Vec2::splat(100.));
        // Grabbing the first region selects it without rearranging or adding.
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(100., 100.),
            egui::pos2(200., 100.),
            false,
        );
        assert_eq!(multi.selected, Some(0));
        assert_eq!(multi.rects[0].min, egui::pos2(150., 50.));
        assert_eq!(multi.preview(&drag), multi.rects);
        // Resize a corner; other regions remain untouched.
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(250., 150.),
            egui::pos2(280., 180.),
            false,
        );
        assert_eq!(multi.rects[0].size(), Vec2::splat(130.));
        assert_eq!(multi.rects[1].min, egui::pos2(300., 300.));
        // Modifier starts an overlapping region rather than moving the first.
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(180., 80.),
            egui::pos2(350., 250.),
            true,
        );
        assert_eq!(multi.rects.len(), 3);
        multi.remove_selected(&mut drag);
        assert_eq!(multi.rects.len(), 2);
        assert!(drag.bounds().is_none());
        assert_eq!(multi.preview(&drag), multi.rects);
    }

    #[test]
    fn multiple_regions_ignore_controls_and_bound_region_count() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(800.));
        let controls = Rect::from_min_size(Pos2::ZERO, Vec2::splat(40.));
        let mut multi = MultiRegions::default();
        let mut drag = RegionDrag::default();
        let events = |a, b| {
            [
                egui::Event::PointerButton {
                    pos: a,
                    pressed: true,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::PointerButton {
                    pos: b,
                    pressed: false,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::COMMAND,
                },
            ]
        };
        multi.update(
            &mut drag,
            &events(egui::pos2(10., 10.), egui::pos2(200., 200.)),
            bounds,
            controls,
            &[],
        );
        assert!(multi.rects.is_empty());
        for _ in 0..40 {
            multi.update(
                &mut drag,
                &events(egui::pos2(100., 100.), egui::pos2(200., 200.)),
                bounds,
                controls,
                &[],
            );
        }
        assert_eq!(multi.rects.len(), 32);
        assert!(!drag.active);
    }

    #[test]
    fn smart_region_clicks_front_window_but_drags_and_handles_remain_free() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let front = Rect::from_min_max(egui::pos2(100., 100.), egui::pos2(500., 400.));
        let back = Rect::from_min_max(egui::pos2(50., 50.), egui::pos2(900., 650.));
        let windows = [(1, front), (2, back)];
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let click = egui::pos2(200., 200.);
        let mut d = RegionDrag::default();
        d.update_smart_region(
            &[button(click, true), button(click, false)],
            bounds,
            Rect::NOTHING,
            &windows,
        );
        assert_eq!(d.bounds(), Some(front));
        // Resizing after a snapped click must not select a different window.
        d.update_smart_region(
            &[
                button(front.right_bottom(), true),
                egui::Event::PointerMoved(egui::pos2(700., 500.)),
                button(egui::pos2(700., 500.), false),
            ],
            bounds,
            Rect::NOTHING,
            &windows,
        );
        assert_eq!(d.bounds().unwrap().max, egui::pos2(700., 500.));
        d = RegionDrag::default();
        d.update_smart_region(
            &[
                button(click, true),
                egui::Event::PointerMoved(egui::pos2(800., 600.)),
                button(egui::pos2(800., 600.), false),
            ],
            bounds,
            Rect::NOTHING,
            &windows,
        );
        assert_eq!(
            d.bounds(),
            Some(Rect::from_min_max(click, egui::pos2(800., 600.)))
        );
        d = RegionDrag::default();
        d.update_smart_region(
            &[button(click, true), button(click, false)],
            bounds,
            front,
            &windows,
        );
        assert!(
            d.bounds().is_none(),
            "Clicks on controls never select windows"
        );
    }

    #[test]
    fn capture_confirmation_stays_near_selection_and_inside_screen() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let size = Vec2::new(600., 62.);
        for selection in [
            Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(300., 200.)),
            Rect::from_min_max(egui::pos2(800., 600.), egui::pos2(990., 690.)),
            bounds,
        ] {
            let panel = Rect::from_min_size(controls_position(bounds, selection, size), size);
            assert!(bounds.contains_rect(panel));
            if selection != bounds {
                assert!(!panel.intersects(selection));
            }
        }
    }

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
