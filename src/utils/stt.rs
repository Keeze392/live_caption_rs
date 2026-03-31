use std::sync::mpsc;
use std::io::Write;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

// loop handling model AI speech to text
pub fn worker(rx: mpsc::Receiver<Vec<f32>>, output_text: &mut String, is_ui_closed: &mut bool) {
    // init
    // load model
    let model_file = "../models/ggml-large-v3-turbo.bin";

    whisper_rs::install_logging_hooks();

    let ctx = match WhisperContext::new_with_params(
        model_file,
        WhisperContextParameters::default()
    ) {
        Ok(res) => res,
        Err(e) => panic!("Error! {e}"),
    };

    let mut speech_to_text = match ctx.create_state() {
        Ok(res) => res,
        Err(e) => panic!("Error! {e}"),
    };

    // start working
    loop {
        if *is_ui_closed {
            break;
        }

        // set params object up (struct)
        let mut params = FullParams::new(SamplingStrategy::Greedy {
            best_of: 5,
        });

        params.set_max_tokens(32);
        params.set_print_progress(false);
        params.set_print_timestamps(false);
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        params.set_no_context(true);

        // get data from channel
        let buffer = match rx.recv() {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Error: {e}");
                continue;
            }
        };

        // is it empty? skip it
        if buffer.is_empty() {
            continue;
        }

        // give to model
        if let Err(e) = speech_to_text.full(params, &buffer) {
            eprintln!("STT error: {e}");
            continue;
        }

        for segment in speech_to_text.as_iter() {
            if is_junk(segment.to_string()) {
                continue;
            }

            let text = match segment.to_str() {
                Ok(s) => s,
                Err(_) => continue,
            };

            add_text(output_text, text);

            print!("{} ", segment);
            std::io::stdout().flush().unwrap();
        }
    }
}

// throw word away if it's whisper decide to stupid choice xD
fn is_junk(text: String) -> bool {
    if text.is_empty() { return true; }

    // common hallucinations from whisper
    const JUNK_WORDS: [&str; 13] = [
        "[BLANK_AUDIO]", "[SILENCE]", "[ Silence ]", "(silence)",
        "[foreign lnaguage]", "(foreign language)",
        "you", "You", "Thank you.", "Thanks for watching!",
        "Bye.", "Bye!", "..."
    ];

    JUNK_WORDS.contains(&&(*text))
}

fn add_text(label_text: &mut String, add_text: &str) {
    if label_text.len() <= 200 {
        label_text.push_str(add_text);
    }
}
