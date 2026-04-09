use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};

use crate::utils::ui::LiveCaptionRs;

use egui::{CentralPanel, widgets};
#[cfg(feature = "osc")]
use egui::TextEdit;
use egui_file_dialog::{FileDialog, Filter};

// Settings Window
impl LiveCaptionRs {
    pub fn settings_window(&mut self, ui: &mut egui::Ui) {
        let transparent_value = self.settings.get_arc_transparent_value();

        let settings_should_open = self.settings.get_arc_settings_should_open_window();

        let arc_select_model = self.settings.get_arc_select_model();
        let arc_select_model_dialog = self.settings.get_arc_select_model_dialog();

        #[cfg(feature = "osc")]
        let arc_osc_output_path = self.settings.get_arc_osc_output_path();
        #[cfg(feature = "osc")]
        let arc_osc_output_port = self.settings.get_arc_osc_output_port();

        let arc_save_history_custom_path = self.settings.get_arc_save_history_custom_path();
        let arc_save_history_dialog = self.settings.get_arc_save_history_dialog();
        let arc_is_enable_save_history = self.settings.get_arc_is_enable_history();

        ui.ctx().show_viewport_deferred(
            egui::ViewportId::from_hash_of("Settings"), 
            egui::ViewportBuilder::default().with_title("Settings"),
            move |ui, _| {
                CentralPanel::default().show_inside(ui, |ui| {
                    // button to open new window for select model file
                    LiveCaptionRs::set_select_model(ui, &arc_select_model, &arc_select_model_dialog);

                    // slider - transparent option
                    let mut value = transparent_value.lock().unwrap();
                    LiveCaptionRs::set_slider_transparent(ui, &mut value);

                    #[cfg(feature = "osc")]
                    LiveCaptionRs::set_text_input_osc_port(ui, &arc_osc_output_port);

                    #[cfg(feature = "osc")]
                    LiveCaptionRs::set_text_input_osc_path(ui, &arc_osc_output_path);

                    LiveCaptionRs::set_save_history_custom_path(ui, &arc_save_history_custom_path, &arc_save_history_dialog);

                    LiveCaptionRs::set_is_enable_save_history(ui, &arc_is_enable_save_history);
                });
                
                // close settings GUI if "x" button is pressed
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    settings_should_open.store(false, Ordering::Relaxed);
                    LiveCaptionRs::save_configuration_file(
                        &arc_select_model,
                        &transparent_value,

                        #[cfg(feature = "osc")]
                        &arc_osc_output_path,
                        #[cfg(feature = "osc")]
                        &arc_osc_output_port,

                        &arc_save_history_custom_path,
                        &arc_is_enable_save_history,
                    );
                }
            });
    }

    // pop up new window for select file model begin with ".bin"
    fn set_select_model(
        ui: &mut egui::Ui,
        select_model: &Arc<Mutex<Option<PathBuf>>>,
        select_model_dialog: &Arc<Mutex<FileDialog>>,
        ) {
        let mut select_window_dialog = select_model_dialog.lock().unwrap();

        ui.label(format!("Select model file: {}",
                select_model
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .to_string_lossy()
            )
        );

        if ui.button("Open").clicked() {
            let dialog = std::mem::take(&mut *select_window_dialog)
                .show_all_files_filter(false)
                .default_file_filter("bin")
                .add_file_filter(
                    "bin",
                    Filter::new(|path: &Path| path.extension().unwrap_or_default() == "bin"))
                .max_selections(1);

            *select_window_dialog = dialog;

            select_window_dialog.pick_file();
        }

        select_window_dialog.update(ui);

        if let Some(path) = select_window_dialog.take_picked() {
            *select_model.lock().unwrap() = Some(path.to_path_buf());
        }
    }

    // set transparent of GUI
    // default: 0.75
    fn set_slider_transparent(ui: &mut egui::Ui, value: &mut f32) {
        ui.add(widgets::Slider::new(value, 0.0..=1.0)
            .step_by(0.05)
        );
    }

    #[cfg(feature = "osc")]
    fn set_text_input_osc_port(ui: &mut egui::Ui, text: &Arc<Mutex<String>>) {
        let mut text_input = text.lock().unwrap().clone();

        ui.label("osc port:");

        ui.add(TextEdit::singleline(&mut text_input));

        *text.lock().unwrap() = text_input;
    }

    #[cfg(feature = "osc")]
    fn set_text_input_osc_path(ui: &mut egui::Ui, text: &Arc<Mutex<String>>) {
        let mut text_input = text.lock().unwrap().clone();

        ui.label("osc path:");

        ui.add(TextEdit::singleline(&mut text_input));

        *text.lock().unwrap() = text_input;
    }

    // select directory for output a History file to that path.
    fn set_save_history_custom_path(
        ui: &mut egui::Ui,
        arc_path: &Arc<Mutex<Option<PathBuf>>>,
        arc_dialog: &Arc<Mutex<FileDialog>>) {

        let mut select_window_dialog = arc_dialog.lock().unwrap();

        ui.label(format!("History custom path: {}",
                arc_path
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .to_string_lossy()
            )
        );

        if ui.button("Open").clicked() {
            select_window_dialog.pick_directory();
        }

        select_window_dialog.update(ui);

        if let Some(path) = select_window_dialog.take_picked() {
            *arc_path.lock().unwrap() = Some(path.to_path_buf());
        }
    }

    fn set_is_enable_save_history(ui: &mut egui::Ui, toggle: &Arc<AtomicBool>) {
        let mut check_toggle = toggle.load(Ordering::Relaxed);

        ui.label(format!("Enable save history?: {check_toggle}"));
        ui.add(egui::Checkbox::new(&mut check_toggle, ""));

        toggle.store(check_toggle, Ordering::Relaxed);
    }
}
