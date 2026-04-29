use std::{path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc}, thread::sleep, time::Duration};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

const RATE: usize = 16000;

const VEC_MINIMUM_SECS: usize = 2;
const VEC_MAXIMUM_SECS: usize = 4;

const VEC_MINIMUM_SAMPLES: usize = RATE * VEC_MINIMUM_SECS;
const VEC_MAXIMUM_SAMPLES: usize = RATE * VEC_MAXIMUM_SECS;

// loop handling model AI speech to text
#[inline]
pub fn worker(rx: mpsc::Receiver<Vec<f32>>,
    output_text: Arc<Mutex<String>>,
    output_text_history: Arc<Mutex<String>>,
    is_ui_closed: Arc<AtomicBool>,
    select_model: Arc<Mutex<Option<PathBuf>>>,
    #[cfg(feature = "osc")]
    output_text_tx: mpsc::SyncSender<String>,
    ) {
    // init
    let mut buffer_live: Vec<f32> = Vec::new();
    let mut model_file: String = "".to_string();
    let mut ctx: Option<WhisperContext>;
    let mut state: Option<WhisperState> = None;

    // this make somehow output to shut up the spam log, what the fuck?
    whisper_rs::install_logging_hooks();

    // start working
    while !is_ui_closed.load(Ordering::Relaxed) {
        // get path
        let new_path_model = {
            let model_path = select_model.lock().unwrap();
            model_path.clone()
        };

        // check if new path for model file, start change to use new model
        if let Some(path) = new_path_model {
            let file = path.to_string_lossy();

            if file != model_file {
                model_file = file.into_owned();

                ctx = match WhisperContext::new_with_params(
                    model_file.clone(),
                    WhisperContextParameters::default(),
                ) {
                    Ok(res) => Some(res),
                    Err(_) => {
                            sleep(Duration::from_millis(500));
                            continue
                        },
                };

                // update the state if ctx did update successfully
                if let Some(inside_ctx) = ctx {
                    state = match inside_ctx.create_state() {
                        Ok(s) => Some(s),
                        Err(e) => {
                            eprintln!("Err -- Creating state failed: {e}");
                            continue;
                        },
                    }
                }
            }
        }

        if let Some(inside_state) = state.as_mut() {
            // get data from channel
            let mut buffer_new: Vec<f32> = match rx.recv() {
                Ok(res) => res,
                Err(_) => break,
            };

            if buffer_new.is_empty() {
                continue;
            }

            // get accurate len for remove old samples excatly number
            // since audio record is 0.5s so we math 0.5 * 4 to get 2s for keep same as minimum chunk
            let buf_sample_len = buffer_new.len() * 4;

            // add new data to buffer_live
            buffer_live.reserve(buffer_new.len());
            buffer_live.append(&mut buffer_new);

            // set params object up (struct)
            let mut params = FullParams::new(SamplingStrategy::Greedy {
                best_of: 5,
            });

            params.set_max_tokens(32);
            params.set_print_progress(false);
            params.set_print_timestamps(false);
            params.set_single_segment(true);
            params.set_translate(true);
            params.set_no_timestamps(true);
            params.set_no_context(true);
            params.set_no_speech_thold(0.7);

            // -----------------------------------------------------------------------
            // buffer manager
            // manage the buffer, keep old samples and add new samples until
            // reach maximum size, it will push old samples chunk to buffer history
            // so it can act like word by word, wihtout need wait for every full chunk
            // -----------------------------------------------------------------------
            // feed to model
            // live
            let mut new_full_text = String::new();
            if buffer_live.len() >= VEC_MINIMUM_SAMPLES {
                new_full_text = task_whisper(
                    inside_state,
                    params.clone(),
                    &mut buffer_live
                );
            }

            // history
            let mut full_text_history = String::new();
            if buffer_live.len() >= VEC_MAXIMUM_SAMPLES {
                full_text_history = task_whisper(
                    inside_state,
                    params,
                    &mut buffer_live.drain(..buf_sample_len).collect()
                );
            }

            // send text output to GUI thread
            if !new_full_text.trim().is_empty() || 
                !full_text_history.trim().is_empty() {

                let mut output = output_text.lock().unwrap();
                *output = new_full_text;

                let mut output_h = output_text_history.lock().unwrap();
                output_h.push_str(&full_text_history);

                // send output text to OSC
                #[cfg(feature = "osc")]
                match output_text_tx.send(format!("{output_h} {output}")) {
                    Ok(()) => (),
                    Err(e) => { println!("Err -- sender channel failed {e}"); break; }
                };
            }

        }
    }
}

fn task_whisper(whisper: &mut WhisperState, params: FullParams, buffer: &mut Vec<f32>) -> String {
    match whisper.full(params, &buffer) {
        Ok(()) => (),
        Err(e) => eprintln!("Err -- running task whisper failed: {e}"),
    };

    let mut output_text = String::new();

    for segment in whisper.as_iter() {
        let text = segment.to_string();

        if is_junk(&text) {
            continue;
        }

        output_text.push_str(&text);
    }

    output_text
}

// throw word away if it's whisper decide to stupid choice xD
fn is_junk(text: &String) -> bool {
    let text_trimmed: String = text.trim().to_lowercase();
    if text_trimmed.is_empty() { return true; }

    // common hallucinations from whisper
    const JUNK_WORDS: [&str; 14] = [
        "[blank_audio]", "[silence]", "[ silence ]", "(silence)",
        "[foreign language]", "(foreign language)",
        "you", "thank you.", "thanks for watching!",
        "bye.", "bye!", "...",
        "*Gunshot*", "Scrrặc"
    ];

    for phrase in JUNK_WORDS.iter() {
        if text_trimmed.contains(phrase) &&
            text_trimmed.len() < 15 {

            return true;
        }
    }

    if text_trimmed
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(3)
        .any(|w| w[0] == w[1] && w[1] == w[2]) {

        return true;
    }

    false
}
