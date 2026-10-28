use super::*;
use crate::capture::{self, DisplayInfo};
use egui::{Color32, Pos2, Rect, Vec2, ViewportCommand, ViewportId};
struct Screen {
    display: DisplayInfo,
    pixels: image::RgbaImage,
    texture: Option<egui::TextureHandle>,
}
#[derive(Default)]
pub(super) struct Desktop {
    loading: Option<mpsc::Receiver<Result<Vec<Screen>, String>>>,
    screens: Vec<Screen>,
    result: Option<mpsc::Sender<Outcome>>,
}
impl Desktop {
    pub fn start(
        &mut self,
        ctx: &egui::Context,
        result: mpsc::Sender<Outcome>,
    ) -> Result<(), String> {
        let context = ctx.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("desktop-color".into())
            .spawn(move || {
                let captured = (|| {
                    let displays = capture::displays()?;
                    let mut screens = Vec::new();
                    let mut pixels = 0_u64;
                    for display in displays {
                        let image = capture::capture_display(display.id, false)?;
                        pixels += u64::from(image.width()) * u64::from(image.height());
                        if pixels > 100_000_000 {
                            return Err(
                                "The connected displays are too large to sample together.".into()
                            );
                        }
                        screens.push(Screen {
                            display,
                            pixels: image,
                            texture: None,
                        });
                    }
                    Ok(screens)
                })();
                let _ = tx.send(captured);
                context.request_repaint();
            })
            .map_err(|e| format!("Could not start screen color selection: {e}"))?;
        self.loading = Some(rx);
        self.result = Some(result);
        Ok(())
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.loading {
            match rx.try_recv() {
                Ok(Ok(screens)) => {
                    self.screens = screens;
                    self.loading = None;
                }
                Ok(Err(error)) => {
                    self.loading = None;
                    if let Some(tx) = self.result.take() {
                        let _ = tx.send(Err(error));
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading = None;
                    if let Some(tx) = self.result.take() {
                        let _ = tx.send(Err("Screen capture stopped.".into()));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut picked = None;
        for screen in &mut self.screens {
            let id = ViewportId::from_hash_of(("desktop-color", screen.display.id));
            let density = ctx.pixels_per_point();
            let origin = Pos2::new(screen.display.x as f32, screen.display.y as f32) / density;
            let size =
                Vec2::new(screen.display.width as f32, screen.display.height as f32) / density;
            ctx.show_viewport_immediate(
                id,
                egui::ViewportBuilder::default()
                    .with_title("Pick screen color — Escape to cancel")
                    .with_decorations(false)
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_position(origin)
                    .with_inner_size(size)
                    .with_window_level(egui::WindowLevel::AlwaysOnTop),
                |ctx, _| {
                    let density = ctx.pixels_per_point();
                    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(
                        Pos2::new(screen.display.x as f32, screen.display.y as f32) / density,
                    ));
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(
                        Vec2::new(screen.display.width as f32, screen.display.height as f32)
                            / density,
                    ));
                    if ctx.input(|i| {
                        i.key_pressed(egui::Key::Escape) || i.viewport().close_requested()
                    }) {
                        picked = Some(None);
                    }
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let texture = screen.texture.get_or_insert_with(|| {
                                ctx.load_texture(
                                    format!("screen-color-{}", screen.display.id),
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [
                                            screen.pixels.width() as usize,
                                            screen.pixels.height() as usize,
                                        ],
                                        screen.pixels.as_raw(),
                                    ),
                                    egui::TextureOptions::NEAREST,
                                )
                            });
                            ui.painter().image(
                                texture.id(),
                                bounds,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                Color32::WHITE,
                            );
                            let response =
                                ui.interact(bounds, ui.id().with("pick"), egui::Sense::click());
                            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                            if let Some(pointer) = response.hover_pos() {
                                crate::capture_magnifier::paint_color(
                                    ui.painter(),
                                    pointer,
                                    bounds,
                                    bounds,
                                    &screen.pixels,
                                );
                                if response.clicked() {
                                    let relative = (pointer - bounds.min) / bounds.size();
                                    let x = (relative.x * screen.pixels.width() as f32)
                                        .floor()
                                        .clamp(0., (screen.pixels.width() - 1) as f32)
                                        as u32;
                                    let y = (relative.y * screen.pixels.height() as f32)
                                        .floor()
                                        .clamp(0., (screen.pixels.height() - 1) as f32)
                                        as u32;
                                    picked = Some(Some(screen.pixels.get_pixel(x, y).0));
                                }
                            }
                        });
                },
            );
        }
        if let Some(color) = picked {
            for screen in &self.screens {
                ctx.send_viewport_cmd_to(
                    ViewportId::from_hash_of(("desktop-color", screen.display.id)),
                    ViewportCommand::Close,
                );
            }
            self.screens.clear();
            if let Some(tx) = self.result.take() {
                let _ = tx.send(Ok(color));
            }
        }
    }
}
