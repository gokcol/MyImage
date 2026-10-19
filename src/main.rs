#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("MyImage currently supports macOS and Windows.");

mod app;
mod appearance;
mod area_pixels;
mod arrow_style;
mod calico_play;
mod canvas;
mod canvas_zoom;
mod capture;
mod capture_magnifier;
mod capture_panel;
mod desktop;
mod dialogs;
mod document;
mod editor;
mod export;
mod export_panel;
mod fill;
mod geometry;
mod image_clipboard;
mod image_loading;
mod library;
mod library_files;
mod library_panel;
mod onboarding;
mod pdf_export;
mod presets;
mod privacy;
mod privacy_preview;
mod recovery;
mod render;
mod runtime;
mod scroll_capture;
mod selection;
mod shapes;
mod stamp_designer;
mod stamps;
mod storage;
mod storage_move;
mod studio;
mod svg_stamp;
mod templates;
mod text_style;
mod texture_cache;

#[cfg(test)]
mod canvas_tests;
#[cfg(test)]
mod fill_tests;
#[cfg(test)]
mod freehand_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod markup_tests;
#[cfg(test)]
mod stamp_tests;
#[cfg(test)]
mod workspace_tests;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("myimage=info,warn"),
    )
    .init();
    runtime::run(std::env::args().any(|arg| arg == "--background"))
}
