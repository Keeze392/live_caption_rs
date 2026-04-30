use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use std::time::Duration;

use eframe::egui;
use egui::{Color32, FontId, RichText, include_image, widgets};
use egui_file_dialog::FileDialog;
use serde::{Deserialize, Serialize};
use serde_json::json;

// Settings GUI
#[derive(Default, Serialize, Deserialize)]
pub struct LiveCaptionSettingsRs {
    // backgroundt transparent
    transparent_value: Arc<Mutex<f32>>,

    // model for speech to text (STT)
    select_model: Arc<Mutex<Option<PathBuf>>>,
    #[serde(skip)]
    select_model_dialog: Arc<Mutex<FileDialog>>,

    // osc for sender, a text from STT
    #[cfg(feature = "osc")]
    osc_output_path: Arc<Mutex<String>>,
    #[cfg(feature = "osc")]
    osc_output_port: Arc<Mutex<String>>,

    // history with toggle
    save_history_custom_path: Arc<Mutex<Option<PathBuf>>>,
    #[serde(skip)]
    save_history_dialog: Arc<Mutex<FileDialog>>,
    is_enable_save_history: Arc<AtomicBool>,

    // audio devices
    #[serde(skip)]
    devices: Arc<Mutex<Vec<String>>>,

    select_device: Arc<Mutex<Option<String>>>,

    // a bool for settings window to appear
    #[serde(skip)]
    should_open_window: Arc<AtomicBool>,
}

// main GUI
#[derive(Default)]
pub struct LiveCaptionRs {
    // for label, text on main window
    speech_to_text: Arc<Mutex<String>>,
    speech_to_text_history: Arc<Mutex<String>>,

    // a bool for tell to other thread follow to close (multi-threading)
    is_ui_closed: Arc<AtomicBool>,

    // settings GUI
    pub settings: LiveCaptionSettingsRs,
}

// settings GUI
impl LiveCaptionSettingsRs {
    pub fn new(
        select_model: Arc<Mutex<Option<PathBuf>>>,
        transparent_value: Arc<Mutex<f32>>,
        #[cfg(feature = "osc")]
        osc_output_path: Arc<Mutex<String>>,
        #[cfg(feature = "osc")]
        osc_output_port: Arc<Mutex<String>>,
        devices: Arc<Mutex<Vec<String>>>,
        select_device: Arc<Mutex<Option<String>>>,
        ) -> Self {
        Self {
            transparent_value: transparent_value,

            select_model: select_model,
            
            #[cfg(feature = "osc")]
            osc_output_path: osc_output_path,
            #[cfg(feature = "osc")]
            osc_output_port: osc_output_port,

            is_enable_save_history: Arc::new(AtomicBool::new(false)),

            should_open_window: Arc::new(AtomicBool::new(false)),

            devices: devices,
            select_device: select_device,

            ..Default::default()
        }
    }

    // get arc data
    pub fn get_arc_select_model(&self) -> Arc<Mutex<Option<PathBuf>>> {
        Arc::clone(&self.select_model)
    }

    pub fn get_arc_select_model_dialog(&self) -> Arc<Mutex<FileDialog>> {
        Arc::clone(&self.select_model_dialog)
    }

    #[cfg(feature = "osc")]
    pub fn get_arc_osc_output_path(&self) -> Arc<Mutex<String>> {
        Arc::clone(&self.osc_output_path)
    }

    #[cfg(feature = "osc")]
    pub fn get_arc_osc_output_port(&self) -> Arc<Mutex<String>> {
        Arc::clone(&self.osc_output_port)
    }

    pub fn get_arc_transparent_value(&self) -> Arc<Mutex<f32>> {
        Arc::clone(&self.transparent_value)
    }

    pub fn get_arc_settings_should_open_window(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_open_window)
    }

    pub fn get_arc_save_history_custom_path(&self) -> Arc<Mutex<Option<PathBuf>>> {
        Arc::clone(&self.save_history_custom_path)
    }

    pub fn get_arc_save_history_dialog(&self) -> Arc<Mutex<FileDialog>> {
        Arc::clone(&self.save_history_dialog)
    }

    pub fn get_arc_is_enable_history(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_enable_save_history)
    }

    pub fn get_arc_devices(&self) -> Arc<Mutex<Vec<String>>> {
        Arc::clone(&self.devices)
    }

    pub fn get_arc_device_selected(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.select_device)
    }
}

impl LiveCaptionRs {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        data_string: Arc<Mutex<String>>,
        data_string_history: Arc<Mutex<String>>,
        #[cfg(feature = "osc")]
        osc_output_path: Arc<Mutex<String>>,
        #[cfg(feature = "osc")]
        osc_output_port: Arc<Mutex<String>>,
        select_model: Arc<Mutex<Option<PathBuf>>>,
        transparent_value: Arc<Mutex<f32>>,
        is_ui_closed: Arc<AtomicBool>,
        arc_devices: Arc<Mutex<Vec<String>>>,
        arc_device_select: Arc<Mutex<Option<String>>>,
        ) -> Self {
            let ctx = &cc.egui_ctx;

            ctx.all_styles_mut(|style| {
                style.override_font_id = Some(FontId::proportional(22.0));
            });
    
            let mut livecaption = Self {
                speech_to_text: data_string,
                speech_to_text_history: data_string_history,

                is_ui_closed: is_ui_closed,

                settings: LiveCaptionSettingsRs::new(
                    select_model,
                    transparent_value,
                    #[cfg(feature = "osc")]
                    osc_output_path,
                    #[cfg(feature = "osc")]
                    osc_output_port,
                    arc_devices,
                    arc_device_select,
                ),

                ..Default::default()
            };

            livecaption.load_configuration_file();

            livecaption
    }

    // Check if output text rows higher than GUI, remove old line.
    // And save the old line to history if is enabled.
    #[inline]
    fn remove_one_wrapped_line(
        ui: &egui::Ui,
        text_shared: &Arc<Mutex<String>>,
        custom_path: &Arc<Mutex<Option<PathBuf>>>,
        is_enable_history: &Arc<AtomicBool>,
        ) {

        let gui_height = ui.available_height();

        // check if available height is high than 0.0, skip it. No remove here.
        if gui_height > 0.0 {
            return;
        }

        let mut text = text_shared.lock().unwrap();

        // get available width in UI
        let gui_width = ui.available_width();

        // get font size
        let font_id = FontId::proportional(22.0);

        let galley = ui.painter().layout(
            text.clone(), 
            font_id, 
            ui.visuals().text_color(),
            gui_width
        );

        // begin process to remove first old line
        let first_line = &galley.rows[0].text();
        let first_line_len = first_line.len();

        // save the delete line to file if is toggle enable
        if is_enable_history.load(Ordering::Relaxed) {
            LiveCaptionRs::save_history_file(text[..first_line_len].to_string(), custom_path.lock().unwrap().clone());
        }

        let new_text = text[first_line_len..].trim_start();
        
        *text = String::from(new_text);
    }

    // save configuration so it will remember all settings
    // resize window, settings gui infonmation etc.
    #[inline]
    pub fn save_configuration_file(
        select_model: &Arc<Mutex<Option<PathBuf>>>,
        transparent_value: &Arc<Mutex<f32>>,
        #[cfg(feature = "osc")]
        osc_output_path: &Arc<Mutex<String>>,
        #[cfg(feature = "osc")]
        osc_output_port: &Arc<Mutex<String>>,
        save_history_custom_path: &Arc<Mutex<Option<PathBuf>>>,
        is_enable_history: &Arc<AtomicBool>,
        select_device: &Arc<Mutex<Option<String>>>,
        ) {
        let config_path: String = match dirs::data_local_dir() {
            Some(p) => p.to_string_lossy().to_string() + "/livecaption/config.json",
            None => { eprintln!("get config path failed"); return; },
        };

        let mut json_build = serde_json::Map::new();

        // save list
        json_build.insert("select_model".into(), json!(select_model));
        json_build.insert("transparent_value".into(), json!(transparent_value));
        json_build.insert("save_history_custom_path".into(), json!(save_history_custom_path));
        json_build.insert("is_enable_save_history".into(), json!(is_enable_history));
        json_build.insert("select_device".into(), json!(select_device));

        #[cfg(feature = "osc")]
        {
            json_build.insert("osc_output_path".into(), json!(osc_output_path));
            json_build.insert("osc_output_port".into(), json!(osc_output_port));
        }

        let file = match File::create(config_path) {
            Ok(f) => f,
            Err(e) => { eprintln!("Failed to create a config file: {e}"); return; },
        };

        let mut writer = BufWriter::new(file);
        match serde_json::to_writer_pretty(&mut writer, &json_build) {
            Ok(()) => (),
            Err(e) => { eprintln!("Failed to write a config file: {e}"); return; }
        }

        match writer.flush() {
            Ok(()) => (),
            Err(e) => { eprintln!("Failed to flush the writer {e}"); return; }
        }
    }
    
    // load only at start up GUI.
    // not sure what kind design for to load struct yet.
    #[inline]
    pub fn load_configuration_file(&mut self) {
        let config_path: String = match dirs::data_local_dir() {
            Some(p) => p.to_string_lossy().to_string() + "/livecaption/config.json",
            None => { eprintln!("Error -- get config path failed"); return; },
        };       

        let file = match fs::read_to_string(config_path) {
            Ok(f) => f,
            Err(_) => { eprintln!("Skipping -- No confing file to load"); return; }
        };

        let unpack_json: LiveCaptionSettingsRs = match serde_json::from_str(&*file) {
            Ok(d) => d,
            Err(e) => { println!("Error -- Trying unpack json failed: {e}"); return; }
        };

        // load list
<<<<<<< Updated upstream
        *self.settings.select_model.lock().unwrap() = unpack_json.select_model.lock().unwrap().take();
        *self.settings.transparent_value.lock().unwrap() = *unpack_json.transparent_value.lock().unwrap();
        *self.settings.save_history_custom_path.lock().unwrap() = unpack_json.save_history_custom_path.lock().unwrap().take();
=======
        *self.settings.select_model.lock().unwrap() = unpack_json.select_model.lock().unwrap().clone();
        *self.settings.transparent_value.lock().unwrap() = unpack_json.transparent_value.lock().unwrap().clone();
        *self.settings.save_history_custom_path.lock().unwrap() = unpack_json.save_history_custom_path.lock().unwrap().clone();
>>>>>>> Stashed changes
        self.settings.is_enable_save_history = unpack_json.is_enable_save_history;
        self.settings.select_device = unpack_json.select_device;

        #[cfg(feature = "osc")]
        {
            *self.settings.osc_output_path.lock().unwrap() = unpack_json.osc_output_path.lock().unwrap().to_string();
            *self.settings.osc_output_port.lock().unwrap() = unpack_json.osc_output_port.lock().unwrap().to_string();
        }
    }

    // create or modify exist history file
    // get all history text into file
    pub fn save_history_file(
        output_text: String,
        custom_path: Option<PathBuf>,
        ) {
        let date = time::OffsetDateTime::now_utc();
        let docs_path = match dirs::document_dir() {
            Some(d) => d,
            None => { eprintln!("Error -- No docs path found, skipping the save, please use custom path."); return; },
        };

        let mut name_with_date = format!(
            "{}/livecaption_histories/livecaption_history_{}_{}_{}.txt",
            docs_path.to_string_lossy(),
            date.year(),
            date.month(),
            date.day()
        );

        // if custom path was set, will use output instead
        if custom_path.is_some() {
            name_with_date = format!("{}/{}_{}_{}.txt",
                custom_path
                    .unwrap_or(docs_path)
                    .to_string_lossy(),
                date.year(),
                date.month(),
                date.day()
            );

        // if custom path was not set, default will triggered to create directory
        // in document if it haven't exist yet
        } else {
            let check_path = format!("{}/livecaption_histories", docs_path.to_string_lossy());

            if !std::path::Path::new(&check_path).exists() {
                match fs::create_dir(check_path) {
                    Ok(()) => (),
                    Err(e) => { eprintln!("Error -- failed to create directory: {e}"); return; }
                };
            }
        }

        let mut file = match fs::File::options()
            .append(true)
            .create(true)
            .open(&name_with_date) {
                Ok(f) => f,
                Err(e) => { eprintln!("Error -- Failed to create history file: {e}"); return; }
            };

        match file.write_all(format!("{}\n", output_text).as_bytes()) {
            Ok(()) => (),
            Err(e) => { eprintln!("Error -- Failed to write into history file: {e}"); return; }
        };
    }
}

impl eframe::App for LiveCaptionRs {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // control the bg transparent
        let bg_color = egui::Rgba::from(visuals.panel_fill);
        let transparent = egui::Rgba::from_rgba_unmultiplied(
            bg_color.r(),
            bg_color.g(),
            bg_color.b(),
            *self.settings.transparent_value.lock().unwrap()
        );

        transparent.to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // get String safety
        let text_shared = self.speech_to_text.lock().unwrap().clone();
        let text_shared_history = self.speech_to_text_history.lock().unwrap().clone();

        let together_text = format!("{text_shared_history} {text_shared}");

        // get original color then connected with control transparent
        let color = egui::Rgba::from_black_alpha(*self.settings.transparent_value.lock().unwrap()).to_srgba_unmultiplied();
        let bg_color = egui::Frame::NONE.fill(egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]));
        
        // left panel with settings button
        egui::Panel::left("left_panel")
            .frame(bg_color)
            .resizable(false)
            .show_separator_line(false)
            .min_size(0.0)
            .show_inside(ui,
            |ui| {
                    let squard_size: f32 = 32.5;
                    let b_settings = ui.add(widgets::Button::image(
                            include_image!("../settings-icon-2.png"))
                                .min_size(egui::vec2(squard_size, squard_size)
                            )
                            .fill(egui::Color32::TRANSPARENT)
                    );
                    
                    if b_settings.clicked() {
                        self.settings.should_open_window.store(true, Ordering::Relaxed);
                    }
        });

        // Label from speech to text
        egui::CentralPanel::default()
            .frame(bg_color)
            .show_inside(ui, |ui| {
            ui.label(RichText::new(together_text.clone())
                .color(Color32::WHITE));

            // check if more than 4 lines, remove one oldest line
            // save one oldest line to history file if enable
            LiveCaptionRs::remove_one_wrapped_line(
                &ui,
                &self.speech_to_text_history,
                &self.settings.save_history_custom_path,
                &self.settings.is_enable_save_history,
            );
        });

        // Settings Window will open if true
        if self.settings.should_open_window.load(Ordering::Relaxed) {
            self.settings_window(ui);
        }

        // limited to 50 fps, think enough.
        ui.request_repaint_after(Duration::from_millis(20));
    }

    // set to true so it can tell other threads should stop if main gui is closed
    fn on_exit(&mut self) {
        self.is_ui_closed.store(true, Ordering::Relaxed);

        if self.settings.is_enable_save_history.load(Ordering::Relaxed) {
            let path = self.settings.save_history_custom_path.lock().unwrap().clone();
            let text_shared = self.speech_to_text.lock().unwrap().clone();
            let text_shared_history = self.speech_to_text_history.lock().unwrap().clone();
            let output_text = format!("{}{}", text_shared_history, text_shared);

            LiveCaptionRs::save_history_file(output_text, path);
        }
    }
}
