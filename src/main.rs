#![allow(dead_code, unused_imports, unused_variables, unused_mut)]

mod app;
mod assets;
mod config;
mod history;
mod network;
mod ssh;
mod ui;

use anyhow::Result;
use eframe::NativeOptions;
use egui::ViewportBuilder;

fn main() -> Result<()> {
    env_logger::init();

    // Build the tokio runtime and enter it so that tokio::spawn works from the
    // egui update thread and Handle::current() is valid in BetterSshApp::new.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();

    let icon = load_icon();

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(assets::APP_NAME)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        assets::APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::BetterSshApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

fn load_icon() -> egui::IconData {
    let image = image::load_from_memory(assets::ICON_PNG)
        .unwrap_or_else(|_| image::DynamicImage::new_rgba8(1, 1));
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    }
}
