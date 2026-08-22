use crate::utils::osc::OSCSender;
use crate::utils::ui_settings::Settings;
use crate::utils::stt::WhisperSTT;
use crate::utils::audio_linux::AudioWorker;

use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool, atomic::Ordering, mpsc},
    time::Duration,
};

use eframe::egui;
use egui::{Color32, FontId, RichText, include_image, widgets};

#[derive(Default)]
pub struct Caption {
    pub current: String,
    pub history: String,
}

// main GUI
#[derive(Default)]
pub struct LiveCaptionRs {
    /// for display caption front of UI
    caption: Arc<Mutex<Caption>>,

    /// a flag for tell to other thread to stop run
    is_ui_closed: Arc<AtomicBool>,

    /// settings GUI
    settings: Arc<Settings>,

    /// OSC for send text to somewhere out of live caption
    osc_sender: OSCSender,

    /// channel from Whisper to GUI
    tx: Option<mpsc::SyncSender<Vec<f32>>>,
}

impl LiveCaptionRs {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.all_styles_mut(|style| {
            style.override_font_id = Some(FontId::proportional(22.0));
        });

        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);

        let livecaption = Self {
            tx: Some(tx),
            settings: Arc::new(Settings::new()),
            osc_sender: OSCSender::new(),
            ..Default::default()
        };

        AudioWorker::get_devices_array(Arc::clone(&livecaption.settings));

        // spawn Whisper in separate Thread
        WhisperSTT::new(
            rx,
            Arc::clone(&livecaption.caption),
            Arc::clone(&livecaption.is_ui_closed),
            Arc::clone(&livecaption.settings),
        )
        .spawn();

        // start audio on startup
        livecaption.spawn_audio_thread();

        livecaption
    }

    fn spawn_audio_thread(&self) {
        if let Some(tx) = &self.tx {
            AudioWorker::new(Arc::clone(&self.settings), Arc::clone(&self.is_ui_closed))
                .spawn(tx.clone());
        }
    }

    /// Check if output text rows higher than GUI, remove old line.
    /// And save the old line to history if enabled.
    #[inline]
    fn remove_one_wrapped_line(&self, ui: &egui::Ui) {
        // check if available height is high than 0.0, skip it. No remove here.
        if ui.available_height() > 0.0 {
            return;
        }

        let text = &mut self.caption.lock().unwrap().history;

        let galley = ui.painter().layout(
            text.clone(),
            FontId::proportional(22.0), // for font size
            ui.visuals().text_color(),
            ui.available_width(),
        );

        // get len of lines in GUI text
        let first_line_len = galley.rows[0].text().len();

        // save the delete line to file if is toggle enable
        if self
            .settings
            .flags
            .is_enable_save_history
            .load(Ordering::Acquire)
        {
            Self::save_history_file(
                text[..first_line_len].to_string(),
                self
                    .settings
                    .data
                    .lock()
                    .unwrap()
                    .save_history_custom_path
                    .clone(),
            );
        }

        let new_text = text[first_line_len..].trim_start();

        *text = String::from(new_text);
    }

    /// create or modify exist history file
    ///  save output_text into file
    pub fn save_history_file(output_text: String, custom_path: Option<PathBuf>) {
        let date = time::OffsetDateTime::now_utc();
        let docs_path = match dirs::document_dir() {
            Some(val) => val,
            None => {
                eprintln!(
                    "Error -- No docs path found, skipping the save, please use custom path."
                );
                return;
            }
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
            name_with_date = format!(
                "{}/{}_{}_{}.txt",
                custom_path.unwrap_or(docs_path).to_string_lossy(),
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
                    Err(e) => {
                        eprintln!("Error -- failed to create directory: {e}");
                        return;
                    }
                };
            }
        }

        let mut file = match fs::File::options()
            .append(true)
            .create(true)
            .open(&name_with_date)
        {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Error -- Failed to create history file: {e}");
                return;
            }
        };

        match file.write_all(format!("{}\n", output_text).as_bytes()) {
            Ok(()) => (),
            Err(e) => {
                eprintln!("Error -- Failed to write into history file: {e}");
                return;
            }
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
            self.settings.data.lock().unwrap().transparent_value,
        );

        transparent.to_array()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // get caption
        let caption_sentences = {
            let caption = self.caption.lock().unwrap();

            format!("{} {}", caption.history.clone(), caption.current.clone())
        };

        // get original color then connected with control transparent
        let color =
            egui::Rgba::from_black_alpha(self.settings.data.lock().unwrap().transparent_value)
                .to_srgba_unmultiplied();
        let bg_color = egui::Frame::NONE.fill(egui::Color32::from_rgba_unmultiplied(
            color[0], color[1], color[2], color[3],
        ));

        // left panel with settings button
        egui::Panel::left("left_panel")
            .frame(bg_color)
            .resizable(false)
            .show_separator_line(false)
            .min_size(0.0)
            .show_inside(ui, |ui| {
                let squard_size: f32 = 32.5;
                let b_settings = ui.add(
                    widgets::Button::image(include_image!("../settings-icon-2.png"))
                        .min_size(egui::vec2(squard_size, squard_size))
                        .fill(egui::Color32::TRANSPARENT),
                );

                if b_settings.clicked() {
                    self.settings
                        .flags
                        .should_open_settings_window
                        .store(true, Ordering::Release);
                }
            });

        // Label from speech to text
        egui::CentralPanel::default()
            .frame(bg_color)
            .show_inside(ui, |ui| {
                ui.label(RichText::new(&caption_sentences).color(Color32::WHITE));

                // check if more than 4 lines, remove one oldest line
                // save one oldest line to history file if enable
                self.remove_one_wrapped_line(&ui);
            });

        // Settings Window will open if true
        if self
            .settings
            .flags
            .should_open_settings_window
            .load(Ordering::Acquire)
        {
            self.settings.settings_window(ui);
        }

        if self
            .settings
            .flags
            .should_save_config
            .load(Ordering::Acquire)
        {
            self.settings.save_configuration_file();

            // this will drop guard once out of scope
            let osc_address = &self.settings.data.lock().unwrap().osc_address;

            // update both path and port regardless if UI settings closed since it only tiny cost
            // which acceptable as it does not happen oftne.
            self.osc_sender.set_path(osc_address.path.clone());
            self.osc_sender.set_port(osc_address.port.clone());

            // set back to false after save config
            self.settings
                .flags
                .should_save_config
                .store(false, Ordering::Release);
        }

        // checking if trigger received that audio needs to restart for target new device
        if self
            .settings
            .flags
            .should_restart_audio
            .load(Ordering::Acquire)
            && self
                .settings
                .flags
                .thread_exited_ready
                .load(Ordering::Acquire)
        {
            println!("DETECT -- Device has changed! -- Audio restarting...");

            // clone the Arc before give to thread
            self.spawn_audio_thread();

            // restart done! set back to false
            self.settings
                .flags
                .should_restart_audio
                .store(false, Ordering::Release);
            self.settings
                .flags
                .thread_exited_ready
                .store(false, Ordering::Release);
        }

        if self.settings.flags.is_enable_osc.load(Ordering::Acquire) {
            // non-vrchat version
            self.osc_sender.send(caption_sentences);

            // vrchat version (unfinish, will add in future)
            //self.osc_sender.send_to_vrc(together_text);
        }

        // limited to 50 fps, think enough. Yes i know this is hard-coded
        ui.request_repaint_after(Duration::from_millis(20));
    }

    fn on_exit(&mut self) {
        // set to true so it can tell other threads should stop if main gui is closed
        self.is_ui_closed.store(true, Ordering::Release);

        // save message leftover when program exit
        if self
            .settings
            .flags
            .is_enable_save_history
            .load(Ordering::Acquire)
        {
            let path = self
                .settings
                .data
                .lock()
                .unwrap()
                .save_history_custom_path
                .clone();
            let text_shared = self.caption.lock().unwrap().current.clone();
            let text_shared_history = self.caption.lock().unwrap().history.clone();
            let output_text = format!("{}{}", text_shared_history, text_shared);

            Self::save_history_file(output_text, path);
        }
    }
}
