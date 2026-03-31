mod utils;

use crate::utils::ui;
use crate::utils::stt;
use crate::utils::audio;

use std::thread;
use std::sync::mpsc;

fn main() {
    env_logger::init();
    // vector mpsc channel
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);
    let mut label_text: String = String::new();
    let mut is_ui_closed: bool = false;

    // audio_data must be 32 bits, 16K and mono for whisper absolute requirement -- done
    // from here for loop begin to become live caption -- done
    // a sample step like each word by word printing? -- unsure
    // also need check if silence/quiet, skip it(continue in loop) to avoid hallucinations -- done
    // (kinda?)

    // add task for handling audio input
    let audio_thread = thread::spawn(move || audio::worker(tx, &mut is_ui_closed));

    // add task for speech to text
    let stt_thread = thread::spawn(move || stt::worker(rx, &mut label_text, &mut is_ui_closed));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_always_on_top()       // for windows
            .with_has_shadow(false)     // for mac os(?)
            .with_transparent(true)
            .with_inner_size([500.0, 100.0])
            .with_min_inner_size([200.0, 25.0])
            .with_max_inner_size([1800.0, 100.0]),
        ..Default::default()
    };

    match eframe::run_native(
        "Live Caption",
        native_options, 
        Box::new(|cc| 
            Ok(
                Box::new(
                    ui::LiveCaptionRs::new(cc)
                )
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
}
