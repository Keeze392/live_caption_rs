use crate::utils::ui;
use std::io::Write;

mod utils;

use eframe::egui;
use log::error;

fn main() {
    env_logger::builder()
        .format(|buf, record| {
            let style = buf.default_level_style(record.level());
            let date = time::OffsetDateTime::now_utc();

            let date_format = format!("{} {}:{}:{}",
                date.date(), date.hour(),
                date.minute(), date.second());

            writeln!(buf, "[{} {}{}{}] {}",
                date_format, style.render(),
                record.level(), style.render_reset(), record.args())
        })
        .filter_level(log::LevelFilter::Info)
        .init();

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
        Err(e) => error!("Failed to run GUI: {e}"),
    };
}
