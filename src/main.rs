mod utils;

use eframe::egui;

#[cfg(feature = "osc")]
use crate::utils::osc;

use crate::utils::ui;
use crate::utils::stt;

#[cfg(target_os = "linux")]
use crate::utils::audio_linux::{audio_worker, get_devices_array};

#[cfg(target_os = "windows")]
use crate::utils::audio_windows::audio_worker;

#[cfg(target_os = "macos")]
use crate::utils::audio_macos::audio_worker;

use std::thread;
use std::sync::{mpsc, Arc, Mutex, atomic::AtomicBool};

fn main() {
    env_logger::init();

    // vector mpsc channel
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);

    #[cfg(feature = "osc")]
    let (output_text_tx, output_text_rx) = mpsc::sync_channel::<String>(8);

    // Arc for safety shared data between Threads
    // bool
    let is_ui_closed_t1 = Arc::new(AtomicBool::new(false));
    let is_ui_closed_t2 = Arc::clone(&is_ui_closed_t1);
    let is_ui_closed_t3 = Arc::clone(&is_ui_closed_t1);
    #[cfg(feature = "osc")]
    let is_ui_closed_t4 = Arc::clone(&is_ui_closed_t1);

    // String
    let text_shared_t2 = Arc::new(Mutex::new(String::new()));
    let text_shared_t3 = Arc::clone(&text_shared_t2);

    let text_shared_history_t2 = Arc::new(Mutex::new(String::new()));
    let text_shared_history_t3 = Arc::clone(&text_shared_history_t2);

    // select model
    let select_model_t2 = Arc::new(Mutex::new(None));
    let select_model_t3 = Arc::clone(&select_model_t2);

    // Transparent
    let transparent_value_t3 = Arc::new(Mutex::new(1.0));

    // audio devices
    let devices_t3 = Arc::new(Mutex::new(Vec::new()));

    get_devices_array(Arc::clone(&devices_t3));

    let device_selected_t1 = Arc::new(Mutex::new(Option::<String>::None));
    let device_selected_t3 = Arc::clone(&device_selected_t1);

    // a bool for restart audio to change select device
    let should_restart_audio_t3 = Arc::new(AtomicBool::new(false));

    // a bool for audio tell main thread that audio thread has exited
    // then main thread can spawn new thread in order to avoid race condition
    let thread_exited_ready_t3 = Arc::new(AtomicBool::new(false));

    // String osc
    #[cfg(feature = "osc")]
    let osc_output_path_t3 = Arc::new(Mutex::new(String::new()));
    #[cfg(feature = "osc")]
    let osc_output_path_t4 = Arc::clone(&osc_output_path_t3);

    #[cfg(feature = "osc")]
    let osc_output_port_t3 = Arc::new(Mutex::new(String::new()));
    #[cfg(feature = "osc")]
    let osc_output_port_t4 = Arc::clone(&osc_output_port_t3);

    // add task for speech to text
    // t2
    let stt_thread = thread::spawn(move || stt::worker(
            rx,
            text_shared_t2,
            text_shared_history_t2,
            is_ui_closed_t2,
            select_model_t2,
            #[cfg(feature = "osc")]
            output_text_tx,
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

    // t4
    #[cfg(feature = "osc")]
    let osc_thread = thread::spawn(move || osc::osc_sender_string(
            output_text_rx,
            is_ui_closed_t4,
            osc_output_path_t4,
            osc_output_port_t4
    ));

    // t3
    match eframe::run_native(
        "Live Caption",
        native_options, 
        Box::new(|cc| 
            Ok(
                Box::new({
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    ui::LiveCaptionRs::new(
                        cc,
                        text_shared_t3,
                        text_shared_history_t3,
                        #[cfg(feature = "osc")]
                        osc_output_path_t3,
                        #[cfg(feature = "osc")]
                        osc_output_port_t3,
                        select_model_t3,
                        transparent_value_t3,
                        is_ui_closed_t3,
                        devices_t3,
                        device_selected_t3,
                        should_restart_audio_t3,
                        tx,
                        thread_exited_ready_t3,
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

    #[cfg(feature = "osc")]
    match osc_thread.join() {
        Ok(()) => (),
        Err(e) => panic!("OSC Thread error: {e:?}"),
    };
}
