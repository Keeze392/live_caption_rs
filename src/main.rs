mod utils;

use eframe::egui;

use crate::utils::ui;

fn main() {
    env_logger::init();

    // main GUI
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_inner_size([800.0, 100.0])
            .with_min_inner_size([50.0, 50.0])
            .with_max_inner_size([1800.0, 300.0]),
        ..Default::default()
    };

    match eframe::run_native(
        "Live Caption",
        native_options,
        Box::new(|cc| {
            Ok(Box::new({
                egui_extras::install_image_loaders(&cc.egui_ctx);
                ui::LiveCaptionRs::new(cc)
            }))
        }),
    ) {
        Ok(()) => (),
        Err(e) => panic!("Error: {e}"),
    };
}
