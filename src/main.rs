mod utils;

use eframe::egui;

use crate::utils::osc;
use crate::utils::ui;
use crate::utils::stt;

#[cfg(target_os = "linux")]
use crate::utils::audio_linux::{audio_worker, get_devices_array};

// not planned to, so idk when for windows and macos.
#[cfg(target_os = "windows")]
use crate::utils::audio_windows::audio_worker;

#[cfg(target_os = "macos")]
use crate::utils::audio_macos::audio_worker;

use crate::osc::OSCSender;
use crate::utils::ui_settings::LiveCaptionSettingsRs;

use std::thread;
use std::sync::{mpsc, Arc, Mutex, atomic::AtomicBool};

fn main() {
    env_logger::init();

    // vector mpsc channel from audio to Whisper
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);

    // bool, if UI is closed, send flag to all thread to stop from loop
    let is_ui_closed = Arc::new(AtomicBool::new(false));

    // String from whisper to UI label
    let text_shared = Arc::new(Mutex::new(String::new()));
    let text_shared_history = Arc::new(Mutex::new(String::new()));

    // select model for whipser
    let select_model = Arc::new(Mutex::new(None));

    // Transparent for UI background
    let transparent_value = Arc::new(Mutex::new(1.0));

    // audio devices
    let devices = Arc::new(Mutex::new(Vec::new()));

    // fill the devices list to choose a device
    get_devices_array(Arc::clone(&devices));
    let device_selected = Arc::new(Mutex::new(Option::<String>::None));

    // a bool for restart audio to change select device
    // it will restart if user choose different a device to listen
    let should_restart_audio = Arc::new(AtomicBool::new(false));

    // a bool for audio tell main thread that audio thread has exited
    // then main thread can spawn new thread in order to avoid race condition
    let thread_exited_ready = Arc::new(AtomicBool::new(false));

    // String osc
    let osc_output_path = Arc::new(Mutex::new(String::new()));
    let osc_output_port = Arc::new(Mutex::new(String::new()));

    // stt - Whisper
    let stt_text_shared = Arc::clone(&text_shared);
    let stt_text_shared_history = Arc::clone(&text_shared_history);
    let stt_is_ui_closed = Arc::clone(&is_ui_closed);
    let stt_select_model = Arc::clone(&select_model);

    let stt_thread = thread::spawn(move || stt::worker(
            rx,
            stt_text_shared,
            stt_text_shared_history,
            stt_is_ui_closed,
            stt_select_model,
    ));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_always_on_top()       // for windows
            .with_has_shadow(false)     // for mac os(?)
            .with_transparent(true)
            .with_inner_size([800.0, 100.0])
            .with_min_inner_size([50.0, 50.0])
            .with_max_inner_size([1800.0, 300.0]),
        ..Default::default()
    };

    let live_caption_settings = LiveCaptionSettingsRs::new(
        Arc::clone(&select_model),
        transparent_value,
        Arc::clone(&osc_output_path),
        Arc::clone(&osc_output_port),
        devices,
        device_selected,
        should_restart_audio,
        thread_exited_ready
    );

    let osc_sender = OSCSender::new(
        &osc_output_path,
        &osc_output_port
    );

    match eframe::run_native(
        "Live Caption",
        native_options, 
        Box::new(|cc| 
            Ok(
                Box::new({
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    ui::LiveCaptionRs::new(
                        cc, 
                        text_shared,
                        text_shared_history,
                        is_ui_closed,
                        tx, 
                        live_caption_settings,
                        osc_sender
                    )
                })
            )
        )
    ) {
        Ok(()) => (),
        Err(e) => panic!("Error: {e}"),
    };

    match stt_thread.join() {
        Ok(()) => (),
        Err(e) => panic!("STT Thread error: {e:?}"),
    };
}
