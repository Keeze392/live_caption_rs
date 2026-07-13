use crate::audio_worker;
use crate::osc::OSCSender;
use crate::utils::ui_settings::LiveCaptionSettingsRs;

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};
use std::time::Duration;
use std::thread;

use eframe::egui;
use egui::{Color32, FontId, RichText, include_image, widgets};

// main GUI
#[derive(Default)]
pub struct LiveCaptionRs {
    // for label, text on main window
    speech_to_text: Arc<Mutex<String>>,
    speech_to_text_history: Arc<Mutex<String>>,

    // a flag for tell to other thread to stop run
    is_ui_closed: Arc<AtomicBool>,

    // settings GUI
    settings: LiveCaptionSettingsRs,

    // OSC for send text to somewhere out of live caption
    osc_sender: OSCSender,

    // temp channel
    tx: Option<mpsc::SyncSender<Vec<f32>>>,
}

impl LiveCaptionRs {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        data_string: Arc<Mutex<String>>,
        data_string_history: Arc<Mutex<String>>,
        is_ui_closed: Arc<AtomicBool>,
        tx: mpsc::SyncSender<Vec<f32>>,
        live_caption_settings: LiveCaptionSettingsRs,
        osc_sender: OSCSender,
        ) -> Self {
            cc.egui_ctx.all_styles_mut(|style| {
                style.override_font_id = Some(FontId::proportional(22.0));
            });
    
            let livecaption = Self {
                speech_to_text: data_string,
                speech_to_text_history: data_string_history,

                is_ui_closed: Arc::clone(&is_ui_closed),

                tx: Some(tx.clone()),

                settings: live_caption_settings,
                osc_sender: osc_sender,

                ..Default::default()
            };

            println!("{:?}", livecaption.settings.select_device
                .lock()
                .unwrap());

            // start audio on startup
            LiveCaptionRs::spawn_audio_thread(
                tx,
                Arc::clone(&livecaption.is_ui_closed),
                Arc::clone(&livecaption.settings.select_device),
                Arc::clone(&livecaption.settings.should_restart_audio),
                Arc::clone(&livecaption.settings.thread_exited_ready),
            );

            livecaption
    }

    fn spawn_audio_thread(
        tx: mpsc::SyncSender<Vec<f32>>,
        is_ui_closed: Arc<AtomicBool>,
        select_device: Arc<Mutex<Option<String>>>,
        should_restart_audio: Arc<AtomicBool>,
        thread_exited_ready: Arc<AtomicBool>,
    ) {

        let _ = thread::spawn(move || audio_worker(
            tx,
            is_ui_closed,
            select_device,
            should_restart_audio,
            thread_exited_ready,
        ));
    }

    // Check if output text rows higher than GUI, remove old line.
    // And save the old line to history if enabled.
    #[inline]
    fn remove_one_wrapped_line(
        ui: &egui::Ui,
        text_shared: &Arc<Mutex<String>>,
        custom_path: &Arc<Mutex<Option<PathBuf>>>,
        is_enable_history: &Arc<AtomicBool>,
        ) {

        // check if available height is high than 0.0, skip it. No remove here.
        if ui.available_height() > 0.0 {
            return;
        }

        let mut text = text_shared.lock().unwrap();

        let galley = ui.painter().layout(
            text.clone(), 
            FontId::proportional(22.0), // for font size
            ui.visuals().text_color(),
            ui.available_width(),
        );

        // get len of lines in GUI text
        let first_line_len = galley.rows[0].text().len();

        // save the delete line to file if is toggle enable
        if is_enable_history.load(Ordering::Acquire) {
            LiveCaptionRs::save_history_file(text[..first_line_len].to_string(), custom_path.lock().unwrap().clone());
        }

        let new_text = text[first_line_len..].trim_start();
        
        *text = String::from(new_text);
    }



    // create or modify exist history file
    // get all history text into file
    pub fn save_history_file(
        output_text: String,
        custom_path: Option<PathBuf>,
        ) {
        let date = time::OffsetDateTime::now_utc();
        let docs_path = match dirs::document_dir() {
            Some(val) => val,
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
                Ok(val) => val,
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
                    self.settings.should_open_settings_window.store(true, Ordering::Release);
                }
        });

        // Label from speech to text
        egui::CentralPanel::default()
            .frame(bg_color)
            .show_inside(ui, |ui| {
            ui.label(RichText::new(&together_text)
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
        if self.settings.should_open_settings_window.load(Ordering::Acquire) {
            self.settings.settings_window(ui);
        }

        if self.settings.should_save_config.load(Ordering::Acquire) {
            self.settings.save_configuration_file();

            // update if change or not, doesn't matter anyway. It may cost a tiny but it's ok
            self.osc_sender.set_path(&self.settings.osc_output_path);
            self.osc_sender.set_port(&self.settings.osc_output_port);

            // set back to false after save config
            self.settings.should_save_config.store(false, Ordering::Release);
        }

        // checking if trigger received that audio needs to restart for target new device
        if self.settings.should_restart_audio.load(Ordering::Acquire) &&
            self.settings.thread_exited_ready.load(Ordering::Acquire) {

            println!("DETECT -- Device has changed! -- Audio restarting...");

            // clone the Arc before give to thread
            LiveCaptionRs::spawn_audio_thread(
                self.tx.clone().expect("Err -- What? not work? how! -- Channel clone failed"),
                Arc::clone(&self.is_ui_closed),
                Arc::clone(&self.settings.select_device),
                Arc::clone(&self.settings.should_restart_audio),
                Arc::clone(&self.settings.thread_exited_ready),
            );

            // restart done! set back to false
            self.settings.should_restart_audio.store(false, Ordering::Release);
            self.settings.thread_exited_ready.store(false, Ordering::Release);
        }

        if self.settings.osc_is_enable.load(Ordering::Acquire) {
            // non-vrchat version
            self.osc_sender.send(together_text);

            // vrchat version (unfinish, will add in future)
            //self.osc_sender.send_to_vrc(together_text);
        }

        // limited to 50 fps, think enough. Yes i know this is hard-coded
        ui.request_repaint_after(Duration::from_millis(20));
    }

    // set to true so it can tell other threads should stop if main gui is closed
    fn on_exit(&mut self) {
        self.is_ui_closed.store(true, Ordering::Release);

        // save message leftover when program exit
        if self.settings.is_enable_save_history.load(Ordering::Acquire) {
            let path = self.settings.save_history_custom_path.lock().unwrap().clone();
            let text_shared = self.speech_to_text.lock().unwrap().clone();
            let text_shared_history = self.speech_to_text_history.lock().unwrap().clone();
            let output_text = format!("{}{}", text_shared_history, text_shared);

            LiveCaptionRs::save_history_file(output_text, path);
        }
    }
}
