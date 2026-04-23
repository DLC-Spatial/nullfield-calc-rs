#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod calc;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 580.0])
            .with_title("Nullfield Calc"),
        ..Default::default()
    };

    eframe::run_native(
        "nullfield-calc",
        native_options,
        Box::new(|cc| Ok(Box::new(app::NullfieldCalcApp::new(cc)))),
    )
}
