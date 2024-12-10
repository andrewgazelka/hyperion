#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![expect(
    clippy::significant_drop_tightening,
    reason = "todo: we should double check no significant drop tightening"
)]

use egui::ViewportBuilder;

mod tri_checkbox;

mod app;
mod shared_state;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size(egui::Vec2::new(1024.0, 768.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Hyperion Packet Inspector",
        native_options,
        Box::new(move |cc| {
            let gui_app = app::GuiApp::new(cc);

            Ok(Box::new(gui_app))
        }),
    )?;

    Ok(())
}
