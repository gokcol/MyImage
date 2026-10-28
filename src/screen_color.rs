//! Explicit, temporary desktop color selection. No screen pixels are saved.
use eframe::egui;
use std::sync::mpsc;
type Outcome = Result<Option<[u8; 4]>, String>;
#[derive(Default)]
pub struct Picker {
    receiver: Option<mpsc::Receiver<Outcome>>,
    #[cfg(target_os = "windows")]
    desktop: windows_picker::Desktop,
}
impl Picker {
    pub fn active(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn start(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.active() {
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};
            let _main = objc2::MainThreadMarker::new()
                .ok_or("Screen color selection requires the main window.")?;
            let context = ctx.clone();
            let handler = block2::RcBlock::new(move |color: *mut NSColor| {
                // AppKit calls once on the main thread; nil means Escape/cancel.
                let result = unsafe { color.as_ref() }
                    .map(|color| {
                        color
                            .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
                            .map(|c| {
                                [
                                    c.redComponent(),
                                    c.greenComponent(),
                                    c.blueComponent(),
                                    c.alphaComponent(),
                                ]
                                .map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
                            })
                            .ok_or_else(|| {
                                "This screen color could not be converted to sRGB.".into()
                            })
                    })
                    .transpose();
                let _ = tx.send(result);
                context.request_repaint();
            });
            // AppKit retains the sampler and copies the block until completion.
            unsafe {
                NSColorSampler::new().showSamplerWithSelectionHandler(&handler);
            }
        }
        #[cfg(target_os = "windows")]
        self.desktop.start(ctx, tx)?;
        self.receiver = Some(rx);
        Ok(())
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        #[cfg(target_os = "windows")]
        self.desktop.show(ctx);
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok(result) => {
                self.receiver = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                None
            }
            Err(_) => {
                self.receiver = None;
                Some(Err(
                    "Screen color selection stopped. Please try again.".into()
                ))
            }
        }
    }
}
#[cfg(target_os = "windows")]
#[path = "screen_color_windows.rs"]
mod windows_picker;
