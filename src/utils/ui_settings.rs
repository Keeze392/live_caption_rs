use std::{io::{BufWriter, Write}, fs::File, path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};

use serde_json::{json};

use egui::{CentralPanel, Response, widgets, TextEdit};
use egui_file_dialog::{FileDialog, Filter};

use serde::{Deserialize, Serialize};

// Settings GUI
#[derive(Default, Serialize, Deserialize)]
pub struct LiveCaptionSettingsRs {
    // backgroundt transparent
    pub transparent_value: Arc<Mutex<f32>>,

    // model for speech to text (STT)
    pub select_model: Arc<Mutex<Option<PathBuf>>>,
    #[serde(skip)]
    pub select_model_dialog: Arc<Mutex<FileDialog>>,

    // osc for sender, a text from STT
    pub osc_is_enable: Arc<AtomicBool>,
    pub osc_output_path: Arc<Mutex<String>>,
    pub osc_output_port: Arc<Mutex<String>>,

    // history with toggle
    pub save_history_custom_path: Arc<Mutex<Option<PathBuf>>>,
    #[serde(skip)]
    pub save_history_dialog: Arc<Mutex<FileDialog>>,
    pub is_enable_save_history: Arc<AtomicBool>,

    // audio devices
    #[serde(skip)]
    pub devices: Arc<Mutex<Vec<String>>>,
    pub select_device: Arc<Mutex<Option<String>>>,

    // restart audio when device changed
    #[serde(skip)]
    pub should_restart_audio: Arc<AtomicBool>,
    #[serde(skip)]
    pub thread_exited_ready: Arc<AtomicBool>,

    // for save config after settings closed
    #[serde(skip)]
    pub should_save_config: Arc<AtomicBool>,

    // a bool for settings window to appear
    #[serde(skip)]
    pub should_open_settings_window: Arc<AtomicBool>,
}

// settings GUI
impl LiveCaptionSettingsRs {
    pub fn new(
        select_model: Arc<Mutex<Option<PathBuf>>>,
        transparent_value: Arc<Mutex<f32>>,
        osc_output_path: Arc<Mutex<String>>,
        osc_output_port: Arc<Mutex<String>>,
        devices: Arc<Mutex<Vec<String>>>,
        select_device: Arc<Mutex<Option<String>>>,
        should_restart_audio: Arc<AtomicBool>,
        thread_exited_ready: Arc<AtomicBool>,
        ) -> Self {
        let mut live_caption_settings = Self {
            transparent_value: transparent_value,

            select_model: select_model,
            
            osc_is_enable: Arc::new(AtomicBool::new(false)),
            osc_output_path: osc_output_path,
            osc_output_port: osc_output_port,

            is_enable_save_history: Arc::new(AtomicBool::new(false)),

            should_open_settings_window: Arc::new(AtomicBool::new(false)),

            devices: devices,
            select_device: select_device,

            should_restart_audio: should_restart_audio,
            thread_exited_ready: thread_exited_ready,

            ..Default::default()
        };

        live_caption_settings.load_configuration_file();

        live_caption_settings
    }

    // save configuration so it will remember all settings
    // resize window, settings gui infonmation etc.
    pub fn save_configuration_file(&self) {
        let config_path: String = match dirs::data_local_dir() {
            Some(val) => val.to_string_lossy().to_string() + "/livecaption/config.json",
            None => { eprintln!("get config path failed"); return; },
        };

        let mut json_build = serde_json::Map::new();

        // save list
        json_build.insert("select_model".into(), json!(self.select_model));
        json_build.insert("transparent_value".into(), json!(self.transparent_value));
        json_build.insert("save_history_custom_path".into(), json!(self.save_history_custom_path));
        json_build.insert("is_enable_save_history".into(), json!(self.is_enable_save_history));
        json_build.insert("select_device".into(), json!(self.select_device));
        json_build.insert("osc_output_path".into(), json!(self.osc_output_path));
        json_build.insert("osc_output_port".into(), json!(self.osc_output_port));
        json_build.insert("osc_is_enable".into(), json!(self.osc_is_enable));

        let file = match File::create(config_path) {
            Ok(val) => val,
            Err(e) => { eprintln!("Failed to create a config file: {e}"); return; },
        };

        // write a file
        let mut writer = BufWriter::new(file);
        match serde_json::to_writer_pretty(&mut writer, &json_build) {
            Ok(()) => (),
            Err(e) => { eprintln!("Failed to write a config file: {e}"); return; }
        }

        match writer.flush() {
            Ok(()) => (),
            Err(e) => { eprintln!("Failed to flush the writer {e}"); return; }
        }

        println!("INFO -- Save configuration successfully");
    }
    
    // load only at start up GUI.
    #[inline]
    pub fn load_configuration_file(&mut self) {
        let config_path: String = match dirs::data_local_dir() {
            Some(val) => val.to_string_lossy().to_string() + "/livecaption/config.json",
            None => { eprintln!("Error -- get config path failed"); return; },
        };       

        let file = match std::fs::read_to_string(config_path) {
            Ok(val) => val,
            Err(_) => { eprintln!("Skipping -- No confing file to load"); return; }
        };

        let unpack_json: LiveCaptionSettingsRs = match serde_json::from_str(&*file) {
            Ok(val) => val,
            Err(e) => { println!("Error -- Trying unpack json failed: {e}"); return; }
        };

        // load list
        *self.select_model.lock().unwrap() = unpack_json.select_model.lock().unwrap().take();
        self.transparent_value = unpack_json.transparent_value;
        self.save_history_custom_path = unpack_json.save_history_custom_path;
        self.is_enable_save_history = unpack_json.is_enable_save_history;
        self.select_device = unpack_json.select_device;
        self.osc_output_path = unpack_json.osc_output_path;
        self.osc_output_port = unpack_json.osc_output_port;
        self.osc_is_enable = unpack_json.osc_is_enable;
    }

    pub fn settings_window(&mut self, ui: &mut egui::Ui) {
        let arc_transparent_value = Arc::clone(&self.transparent_value);

        let arc_should_open_settings_window = Arc::clone(&self.should_open_settings_window);

        let arc_select_model = Arc::clone(&self.select_model);
        let arc_select_model_dialog = Arc::clone(&self.select_model_dialog);

        let arc_osc_is_enable = Arc::clone(&self.osc_is_enable);
        let arc_osc_output_path = Arc::clone(&self.osc_output_path);
        let arc_osc_output_port = Arc::clone(&self.osc_output_port);

        let arc_save_history_custom_path = Arc::clone(&self.save_history_custom_path);
        let arc_save_history_dialog = Arc::clone(&self.save_history_dialog);
        let arc_is_enable_save_history = Arc::clone(&self.is_enable_save_history);

        let arc_devices = Arc::clone(&self.devices);
        let arc_device_selected = Arc::clone(&self.select_device);
        let arc_should_restart_audio = Arc::clone(&self.should_restart_audio);

        let arc_should_save_config = Arc::clone(&self.should_save_config);

        ui.ctx().show_viewport_deferred(
            egui::ViewportId::from_hash_of("Settings"), 
            egui::ViewportBuilder::default().with_title("Settings"),
            move |ui, _| {
                CentralPanel::default().show_inside(ui, |ui| {
                    // devices list to pick one device for listening
                    LiveCaptionSettingsRs::set_combobox_devices(
                        ui,
                        &arc_devices,
                        &arc_device_selected,
                        &arc_should_restart_audio
                    );

                    ui.separator();

                    // button to open new window for select model file
                    LiveCaptionSettingsRs::set_select_model(
                        ui,
                        &arc_select_model,
                        &arc_select_model_dialog
                    );

                    ui.separator();

                    // slider - transparent option
                    let mut value = arc_transparent_value.lock().unwrap();
                    LiveCaptionSettingsRs::set_slider_transparent(ui, &mut value);

                    ui.separator();

                    // OSC - expose the output text to outside
                    LiveCaptionSettingsRs::toggle_osc(ui, &arc_osc_is_enable);
                    LiveCaptionSettingsRs::set_text_input_osc_port(ui, &arc_osc_output_port);
                    LiveCaptionSettingsRs::set_text_input_osc_path(ui, &arc_osc_output_path);
                    
                    ui.separator();

                    // save output text to history file
                    LiveCaptionSettingsRs::set_save_history_custom_path(
                        ui,
                        &arc_save_history_custom_path,
                        &arc_save_history_dialog
                    );

                    LiveCaptionSettingsRs::set_is_enable_save_history(ui, &arc_is_enable_save_history);

                    ui.separator();
                });
                
                // close settings GUI if "x" button is pressed
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    arc_should_open_settings_window.store(false, Ordering::Release);
                    arc_should_save_config.store(true, Ordering::Release);
               }
            }
        );
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
            should_restart_audio.store(true, Ordering::Release);
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

    #[inline]
    fn toggle_osc(ui: &mut egui::Ui, toggle: &Arc<AtomicBool>) {
        ui.label("Enable OSC?");
        let mut toggle_bool = toggle.load(Ordering::Acquire);

        ui.checkbox(&mut toggle_bool, "OSC");

        toggle.store(toggle_bool, Ordering::Release);
    }

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
        let mut on = toggle.load(Ordering::Acquire);

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

        toggle.store(on, Ordering::Release);

        response
    }
}
