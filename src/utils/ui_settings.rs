use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use egui::{CentralPanel, TextEdit, widgets};
use egui_file_dialog::{FileDialog, Filter};

use serde::{Deserialize, Serialize};

use crate::utils::audio_linux::AudioWorker;
use crate::utils::osc::OSCAddress;

/// flags for tell threads what to do
/// Should used with Arc
#[derive(Default, Serialize, Deserialize)]
pub struct Flags {
    pub is_enable_osc: AtomicBool,

    pub is_enable_save_history: AtomicBool,

    // send flag restart audio when device changed
    #[serde(skip)]
    pub should_restart_audio: AtomicBool,
    #[serde(skip)]
    pub thread_exited_ready: AtomicBool,

    // for save config after settings closed
    #[serde(skip)]
    pub should_save_config: AtomicBool,

    // a bool for settings window to appear
    #[serde(skip)]
    pub should_open_settings_window: AtomicBool,
}

/// Settings for Live caption GUI itself
#[derive(Default, Serialize, Deserialize)]
pub struct Data {
    // backgroundt transparent
    pub transparent_value: f32,

    // model for speech to text (STT)
    pub select_model: Option<PathBuf>,
    #[serde(skip)]
    pub select_model_dialog: FileDialog,

    // osc for sender, a text from STT
    pub osc_address: OSCAddress,

    // history
    pub save_history_custom_path: Option<PathBuf>,
    #[serde(skip)]
    pub save_history_dialog: FileDialog,

    // audio devices
    #[serde(skip)]
    pub devices: Vec<String>,
    pub select_device: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub flags: Arc<Flags>,
    pub data: Arc<Mutex<Data>>,
}

// settings GUI
impl Settings {
    pub fn new() -> Self {
        // will load any data otherwise return default if file isn't exist or error
        let settings = Self::load_configuration_file();

        AudioWorker::get_devices_array(Arc::clone(&settings.data));

        settings
    }

    /// save configuration so it will remember all settings
    /// resize window, settings gui infonmation etc.
    pub fn save_configuration_file(&self) {
        let config_path: String = match dirs::data_local_dir() {
            Some(val) => val.to_string_lossy().to_string() + "/livecaption/config.json",
            None => {
                eprintln!("get config path failed");
                return;
            }
        };

        let json = serde_json::json!(&self);

        let file = match File::create(config_path) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Failed to create a config file: {e}");
                return;
            }
        };

        // write a file
        let mut writer = BufWriter::new(file);
        match serde_json::to_writer_pretty(&mut writer, &json) {
            Ok(()) => (),
            Err(e) => {
                eprintln!("Failed to write a config file: {e}");
                return;
            }
        }

        match writer.flush() {
            Ok(()) => (),
            Err(e) => {
                eprintln!("Failed to flush the writer {e}");
                return;
            }
        }

        println!("INFO -- Save configuration successfully");
    }

    /// load only at start up GUI.
    #[inline]
    fn load_configuration_file() -> Settings {
        let config_path: String = match dirs::data_local_dir() {
            Some(val) => val.to_string_lossy().to_string() + "/livecaption/config.json",
            None => {
                eprintln!("Error -- get config path failed");
                return Self::default();
            }
        };

        let file = match std::fs::read_to_string(config_path) {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Skipping -- No confing file to load");
                return Self::default();
            }
        };

        let unpack_json: Settings = match serde_json::from_str(&*file) {
            Ok(val) => val,
            Err(e) => {
                println!("Error -- Trying unpack json failed: {e}");
                return Self::default();
            }
        };

        unpack_json
    }

    pub fn settings_window(&self, ui: &mut egui::Ui) {
        let data = Arc::clone(&self.data);
        let flags = Arc::clone(&self.flags);

        ui.ctx().show_viewport_deferred(
            egui::ViewportId::from_hash_of("Settings"),
            egui::ViewportBuilder::default().with_title("Settings"),
            move |ui, _| {
                CentralPanel::default().show_inside(ui, |ui| {
                    let mut data_guard = data.lock().unwrap();

                    // devices list to pick one device for listening
                    Self::set_combobox_devices(ui, &mut data_guard, &flags);

                    ui.separator();

                    // button to open new window for select model file
                    Self::set_select_model(ui, &mut data_guard);

                    ui.separator();

                    // slider - transparent option
                    Self::set_slider_transparent(ui, &mut data_guard);

                    ui.separator();

                    // OSC - expose the output text to outside
                    Self::toggle_osc(ui, &flags);
                    Self::set_text_input_osc_port(ui, &mut data_guard);
                    Self::set_text_input_osc_path(ui, &mut data_guard);

                    ui.separator();

                    Self::toggle_is_enable_save_history(ui, &flags);

                    // save output text to history file
                    Self::set_save_history_custom_path(ui, &mut data_guard);

                    ui.separator();
                });

                // close settings GUI if "x" button is pressed
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    flags
                        .should_open_settings_window
                        .store(false, Ordering::Release);
                    flags.should_save_config.store(true, Ordering::Release);

                    // refresh in case if user going open window again to see if devices get refresh
                    AudioWorker::get_devices_array(Arc::clone(&data));
                }
            },
        );
    }

    /// get audio devices and show combobox for user to pick a choice.
    /// this will refresh every time settings is open incase if user plug something
    #[inline]
    fn set_combobox_devices(ui: &mut egui::Ui, data: &mut Data, flags: &Arc<Flags>) {
        ui.label("Audio Devices, select a device for what should listening on.");

        let mut selected = data.select_device.clone();
        let devices = &data.devices;
        let before = selected.clone();

        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_label("")
                .selected_text(format!(
                    "{}",
                    selected.clone().unwrap_or("None".to_string())
                ))
                .show_ui(ui, |ui| {
                    for device in devices {
                        ui.selectable_value(
                            &mut selected,
                            Some(device.clone()),
                            format!("{}", device),
                        );
                    }
                })
        });

        // has device changed? send trigger restart the audio
        if selected != before {
            flags.should_restart_audio.store(true, Ordering::Release);
        }

        data.select_device = selected;
    }

    /// pop up new window for select file model begin with ".bin"
    #[inline]
    fn set_select_model(ui: &mut egui::Ui, data: &mut Data) {
        ui.label("Select model to load Speech to text AI");

        ui.horizontal_wrapped(|ui| {
            if ui.button("Open").clicked() {
                let dialog = std::mem::take(&mut data.select_model_dialog)
                    .show_all_files_filter(false)
                    .default_file_filter("bin")
                    .add_file_filter(
                        "bin",
                        Filter::new(|path: &Path| path.extension().unwrap_or_default() == "bin"),
                    )
                    .max_selections(1);

                data.select_model_dialog = dialog;

                data.select_model_dialog.pick_file();
            }
        });
        ui.label(format!(
            "model: {}",
            data.select_model
                .as_ref()
                .unwrap_or(&PathBuf::from("None"))
                .file_name()
                .unwrap()
                .to_string_lossy()
        ));

        data.select_model_dialog.update(ui);

        if let Some(path) = data.select_model_dialog.take_picked() {
            data.select_model = Some(path.to_path_buf());
        }
    }

    /// set transparent of GUI
    /// default: 0.00
    #[inline]
    fn set_slider_transparent(ui: &mut egui::Ui, data: &mut Data) {
        ui.label("Transparent for background");

        ui.horizontal_wrapped(|ui| {
            ui.label("Transparent:");
            ui.add(widgets::Slider::new(&mut data.transparent_value, 0.0..=1.0).step_by(0.05));
        });
    }

    #[inline]
    fn toggle_osc(ui: &mut egui::Ui, flags: &Arc<Flags>) {
        ui.label("Enable OSC?");
        let mut toggle_bool = flags.is_enable_osc.load(Ordering::Acquire);

        ui.checkbox(&mut toggle_bool, "OSC");

        flags.is_enable_osc.store(toggle_bool, Ordering::Release);
    }

    #[inline]
    fn set_text_input_osc_port(ui: &mut egui::Ui, data: &mut Data) {
        ui.label("OSC expose the output text to outside.");
        let mut port = data.osc_address.port.clone();

        ui.horizontal_wrapped(|ui| {
            ui.label("osc port:");

            ui.add(TextEdit::singleline(&mut port));
        });

        data.osc_address.port = port;
    }

    #[inline]
    fn set_text_input_osc_path(ui: &mut egui::Ui, data: &mut Data) {
        let mut path = data.osc_address.path.clone();

        ui.horizontal_wrapped(|ui| {
            ui.label("osc path:");

            ui.add(TextEdit::singleline(&mut path));
        });

        data.osc_address.path = path;
    }

    /// select directory for output a History file to that path.
    #[inline]
    fn set_save_history_custom_path(ui: &mut egui::Ui, data: &mut Data) {
        ui.label("If you wish to save output text as history, you can enable here.");

        if ui.button("Open").clicked() {
            data.save_history_dialog.pick_directory();
        }

        ui.label(format!(
            "Custom path: {}",
            data.save_history_custom_path
                .as_ref()
                .unwrap_or(&PathBuf::from("None"))
                .to_string_lossy()
        ));

        data.save_history_dialog.update(ui);

        if let Some(path) = data.save_history_dialog.take_picked() {
            data.save_history_custom_path = Some(path.to_path_buf());
        }
    }

    #[inline]
    fn toggle_is_enable_save_history(ui: &mut egui::Ui, flags: &Arc<Flags>) {
        ui.label(format!("Enable history?"));

        let mut toggle_bool = flags.is_enable_save_history.load(Ordering::Acquire);

        ui.checkbox(&mut toggle_bool, "History");

        flags
            .is_enable_save_history
            .store(toggle_bool, Ordering::Release);
    }
}
