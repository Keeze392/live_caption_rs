use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};

use crate::utils::ui::LiveCaptionRs;

use egui::{CentralPanel, Response, widgets};
#[cfg(feature = "osc")]
use egui::TextEdit;
use egui_file_dialog::{FileDialog, Filter};

// Settings Window
impl LiveCaptionRs {
    pub fn settings_window(&mut self, ui: &mut egui::Ui) {
        let arc_transparent_value = self.settings.get_arc_transparent_value();

        let arc_settings_should_open = self.settings.get_arc_settings_should_open_window();

        let arc_select_model = self.settings.get_arc_select_model();
        let arc_select_model_dialog = self.settings.get_arc_select_model_dialog();

        #[cfg(feature = "osc")]
        let arc_osc_output_path = self.settings.get_arc_osc_output_path();
        #[cfg(feature = "osc")]
        let arc_osc_output_port = self.settings.get_arc_osc_output_port();

        let arc_save_history_custom_path = self.settings.get_arc_save_history_custom_path();
        let arc_save_history_dialog = self.settings.get_arc_save_history_dialog();
        let arc_is_enable_save_history = self.settings.get_arc_is_enable_history();

        let arc_devices = self.settings.get_arc_devices();
        let arc_device_selected = self.settings.get_arc_device_selected();
        let arc_should_restart_audio = self.settings.get_arc_should_restart_audio();

        ui.ctx().show_viewport_deferred(
            egui::ViewportId::from_hash_of("Settings"), 
            egui::ViewportBuilder::default().with_title("Settings"),
            move |ui, _| {
                CentralPanel::default().show_inside(ui, |ui| {
                    // devices list to pick one device for listening
                    LiveCaptionRs::set_combobox_devices(
                        ui,
                        &arc_devices,
                        &arc_device_selected,
                        &arc_should_restart_audio
                    );

                    ui.separator();

                    // button to open new window for select model file
                    LiveCaptionRs::set_select_model(
                        ui,
                        &arc_select_model,
                        &arc_select_model_dialog
                    );

                    ui.separator();

                    // slider - transparent option
                    let mut value = arc_transparent_value.lock().unwrap();
                    LiveCaptionRs::set_slider_transparent(ui, &mut value);

                    ui.separator();

                    #[cfg(feature = "osc")]
                    {
                        // OSC - expose the output text to outside
                        LiveCaptionRs::set_text_input_osc_port(ui, &arc_osc_output_port);

                        LiveCaptionRs::set_text_input_osc_path(ui, &arc_osc_output_path);
                        
                        ui.separator();
                    }

                    // save output text to history file
                    LiveCaptionRs::set_save_history_custom_path(
                        ui,
                        &arc_save_history_custom_path,
                        &arc_save_history_dialog
                    );

                    LiveCaptionRs::set_is_enable_save_history(ui, &arc_is_enable_save_history);

                    ui.separator();
                });
                
                // close settings GUI if "x" button is pressed
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    arc_settings_should_open.store(false, Ordering::Relaxed);
                    LiveCaptionRs::save_configuration_file(
                        &arc_select_model,
                        &arc_transparent_value,

                        #[cfg(feature = "osc")]
                        &arc_osc_output_path,
                        #[cfg(feature = "osc")]
                        &arc_osc_output_port,

                        &arc_save_history_custom_path,
                        &arc_is_enable_save_history,

                        &arc_device_selected,
                    );
                    
                    println!("INFO -- Configuration saved successfully!");
               }
            });
    }

    // get audio devices and show combobox for user to pick a choice.
    // this will refresh every time settings is open incase if user plug something
    #[inline]
    fn set_combobox_devices(
        ui: &mut egui::Ui,
        arc_devices: &Arc<Mutex<Vec<String>>>,
        arc_selected: &Arc<Mutex<Option<String>>>,
        should_restart_audio: &Arc<AtomicBool>,
    ) {
        ui.label("Audio Devices, select a device for what should listening on.");

        let mut selected = arc_selected.lock().unwrap().clone();
        let devices = arc_devices.lock().unwrap().clone();
        let before = selected.clone();

        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_label("")
                .selected_text(format!("{}",
                    selected
                        .clone()
                        .unwrap_or("None".to_string())))
                .show_ui(ui, |ui| {
                    for device in devices {
                        ui.selectable_value(&mut selected, Some(device.clone()), format!("{}", device));
                    }
                })
        });

        // has device changed? send trigger restart the audio
        if selected != before {
            should_restart_audio.store(true, Ordering::Relaxed);
        }

        *arc_selected.lock().unwrap() = selected;
    }

    // pop up new window for select file model begin with ".bin"
    #[inline]
    fn set_select_model(
        ui: &mut egui::Ui,
        select_model: &Arc<Mutex<Option<PathBuf>>>,
        select_model_dialog: &Arc<Mutex<FileDialog>>,
        ) {
        let mut select_window_dialog = select_model_dialog.lock().unwrap();

        ui.label("Select model to load Speech to text AI");

        ui.horizontal_wrapped(|ui| {
                if ui.button("Open").clicked() {
                    let dialog = std::mem::take(&mut *select_window_dialog)
                        .show_all_files_filter(false)
                        .default_file_filter("bin")
                        .add_file_filter(
                            "bin",
                            Filter::new(|path: &Path| path
                                .extension()
                                .unwrap_or_default() == "bin"))
                        .max_selections(1);

                    *select_window_dialog = dialog;

                    select_window_dialog.pick_file();
                }
        });
            ui.label(format!("model: {}", 
                    select_model
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap_or(&PathBuf::from("None"))
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
            ));

        select_window_dialog.update(ui);

        if let Some(path) = select_window_dialog.take_picked() {
            *select_model.lock().unwrap() = Some(path.to_path_buf());
        }
    }

    // set transparent of GUI
    // default: 0.75
    #[inline]
    fn set_slider_transparent(ui: &mut egui::Ui, value: &mut f32) {
        ui.label("Transparent for background");

        ui.horizontal_wrapped(|ui| {
            ui.label("Transparent:");
            ui.add(widgets::Slider::new(value, 0.0..=1.0)
                .step_by(0.05)
            );
        });
    }

    #[cfg(feature = "osc")]
    #[inline]
    fn set_text_input_osc_port(ui: &mut egui::Ui, text: &Arc<Mutex<String>>) {
        ui.label("OSC expose the output text to outside.");
        let mut text_input = text.lock().unwrap().clone();

        ui.horizontal_wrapped(|ui| {
            ui.label("osc port:");

            ui.add(TextEdit::singleline(&mut text_input));
        });

            *text.lock().unwrap() = text_input;
    }

    #[cfg(feature = "osc")]
    #[inline]
    fn set_text_input_osc_path(ui: &mut egui::Ui, text: &Arc<Mutex<String>>) {
        let mut text_input = text.lock().unwrap().clone();

        ui.horizontal_wrapped(|ui| {
            ui.label("osc path:");

            ui.add(TextEdit::singleline(&mut text_input));
        });

        *text.lock().unwrap() = text_input;
    }

    // select directory for output a History file to that path.
    #[inline]
    fn set_save_history_custom_path(
        ui: &mut egui::Ui,
        arc_path: &Arc<Mutex<Option<PathBuf>>>,
        arc_dialog: &Arc<Mutex<FileDialog>>) {

        let mut select_window_dialog = arc_dialog.lock().unwrap();

        ui.label("If you wish to save output text as history, you can enable here.");

        if ui.button("Open").clicked() {
            select_window_dialog.pick_directory();
        }

        ui.label(format!("Custom path: {}",
                arc_path
                .lock()
                .unwrap()
                .as_ref()
                .unwrap_or(&PathBuf::from("None"))
                .to_string_lossy()
            )
        );

        select_window_dialog.update(ui);

        if let Some(path) = select_window_dialog.take_picked() {
            *arc_path.lock().unwrap() = Some(path.to_path_buf());
        }
    }

    // a custom switch toggle, copied from egui example about switch toggle. (why they didn't put
    // into his widget!? :V)
    #[inline]
    fn set_is_enable_save_history(ui: &mut egui::Ui, toggle: &Arc<AtomicBool>) -> Response {
        let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        let mut on = toggle.load(Ordering::Relaxed);

        ui.label(format!("Enable history:"));

        if response.clicked() {
            on = !on;
            response.mark_changed();
        }

        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), on, "")
        });

        if ui.is_rect_visible(rect) {
            let how_on = ui.ctx().animate_bool_responsive(response.id, on);
            let visuals = ui.style().interact_selectable(&response, on);
            let rect = rect.expand(visuals.expansion);
            let radius = 0.5 * rect.height();

            ui.painter().rect(
                rect,
                radius,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );

            let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
            let center = egui::pos2(circle_x, rect.center().y);

            ui.painter()
                .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
        }

        toggle.store(on, Ordering::Relaxed);

        response
    }
}
