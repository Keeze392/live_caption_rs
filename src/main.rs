mod utils;

use eframe::egui;

//use crate::utils::audio_cpal;
#[cfg(feature = "osc")]
use crate::utils::osc;

use crate::utils::ui;
use crate::utils::stt;
use crate::utils::audio;

use std::thread;
use std::sync::{mpsc, Arc, Mutex, atomic::AtomicBool};

fn main() {
    env_logger::init();

    // vector mpsc channel
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);

    #[cfg(feature = "osc")]
    let (output_text_tx, output_text_rx) = mpsc::sync_channel::<String>(8);
    #[cfg(feature = "osc")]
    println!("osc on!");

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

    // String osc
    #[cfg(feature = "osc")]
    let osc_output_path_t3 = Arc::new(Mutex::new(String::new()));
    #[cfg(feature = "osc")]
    let osc_output_path_t4 = Arc::clone(&osc_output_path_t3);

    #[cfg(feature = "osc")]
    let osc_output_port_t3 = Arc::new(Mutex::new(String::new()));
    #[cfg(feature = "osc")]
    let osc_output_port_t4 = Arc::clone(&osc_output_port_t3);

    // add task for handling audio input
    // t1
    let audio_thread = thread::spawn(move || audio::worker(tx, is_ui_closed_t1));

    //let (tx_dummy, _) = mpsc::sync_channel::<Vec<f32>>(16);
    //let audio_thread_dummy = thread::spawn(move || audio_cpal::worker(tx_dummy));

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

    #[cfg(feature = "osc")]
    let osc_thread = thread::spawn(move || osc::osc_sender_string(
            output_text_rx,
            is_ui_closed_t4,
            osc_output_path_t4,
            osc_output_port_t4
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
                    )
                })
            )
        )
    ) {
        Ok(()) => (),
        Err(e) => panic!("Error: {e}"),
    };
    
    match audio_thread.join() {
        Ok(()) => (),
        Err(_) => panic!("Audio Thread error"),
    };

    match stt_thread.join() {
        Ok(()) => (),
        Err(_) => panic!("STT Thread error"),
    };

    #[cfg(feature = "osc")]
    match osc_thread.join() {
        Ok(()) => (),
        Err(_) => panic!("OSC Thread error"),
    };

    //audio_thread_dummy.join().unwrap();
}
