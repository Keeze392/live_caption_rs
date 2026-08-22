use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle, sleep},
    time::Duration,
};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::utils::{ui::Caption, ui_settings::Settings};

const RATE: usize = 16000;

// adjust time if you wish short or long
const VEC_MINIMUM_SECS: usize = 2;
const VEC_MAXIMUM_SECS: usize = 4;

const VEC_MINIMUM_SAMPLES: usize = RATE * VEC_MINIMUM_SECS;
const VEC_MAXIMUM_SAMPLES: usize = RATE * VEC_MAXIMUM_SECS;

pub struct WhisperSTT {
    rx: mpsc::Receiver<Vec<f32>>,
    caption: Arc<Mutex<Caption>>,
    is_ui_closed: Arc<AtomicBool>,
    settings: Arc<Settings>,

    buffer_live: Vec<f32>,
    model_file: PathBuf,
    ctx: Option<WhisperContext>,
    state: Option<WhisperState>,
}

impl WhisperSTT {
    pub fn new(
        rx: mpsc::Receiver<Vec<f32>>,
        caption: Arc<Mutex<Caption>>,
        is_ui_closed: Arc<AtomicBool>,
        settings: Arc<Settings>,
    ) -> Self {
        // for stop spam logs for whatever reasons by add those
        // though docs said it will logs if add, this seem opposite way.
        whisper_rs::install_logging_hooks();

        Self {
            rx: rx,
            caption: caption,
            is_ui_closed: is_ui_closed,
            settings: settings,

            buffer_live: Vec::new(),
            model_file: PathBuf::new(),
            ctx: None,
            state: None,
        }
    }

    // Spawn task loop in separate Thread
    pub fn spawn(self) -> JoinHandle<()> {
        thread::spawn(move || {
            self.run();
        })
    }

    /// Run the Whisper to start listening to audio and process convert audio to text then
    /// send to channel other GUI display.
    fn run(mut self) {
        // start working
        while !self.is_ui_closed.load(Ordering::Acquire) {
            // get path
            let new_path_model = self.settings.data.lock().unwrap().select_model.clone();

            // check if path and file is valid and change model if is different name
            if let Some(path) = new_path_model
                && path != self.model_file
            {
                // check if file is available in that path
                if !std::fs::exists(&path).unwrap_or(true) {
                    eprintln!(
                        "No file found in this path: {}\nPlease go to settings to pick file path for stt model",
                        &path.to_string_lossy()
                    );
                    sleep(Duration::from_secs(1));
                    continue;
                }

                self.ctx = match WhisperContext::new_with_params(
                    &path,
                    WhisperContextParameters::default(),
                ) {
                    Ok(res) => Some(res),
                    Err(_) => {
                        sleep(Duration::from_millis(500));
                        continue;
                    }
                };

                self.model_file = path;

                // update the state if ctx did update successfully
                if let Some(inside_ctx) = &self.ctx {
                    self.state = match inside_ctx.create_state() {
                        Ok(s) => Some(s),
                        Err(e) => {
                            eprintln!("Err -- Creating state failed: {e}");
                            continue;
                        }
                    }
                }
            }

            if let Some(inside_state) = self.state.as_mut() {
                // get data from channel
                let mut buffer_new: Vec<f32> = match self.rx.recv() {
                    Ok(res) => res,
                    Err(_) => break,
                };

                if buffer_new.is_empty() {
                    continue;
                }

                // set params object up (struct)
                let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });

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

                // get accurate len for remove old samples excatly number
                // since audio record is 0.5s so we math 0.5 * 4 to get 2s for keep same as minimum chunk
                let buf_sample_len = buffer_new.len() * 4;

                // add new data to buffer_live
                self.buffer_live.append(&mut buffer_new);

                // feed to model
                // live
                let mut new_full_text = String::new();
                if self.buffer_live.len() >= VEC_MINIMUM_SAMPLES {
                    new_full_text =
                        Self::task_whisper(inside_state, params.clone(), &mut self.buffer_live);
                }

                // history
                let mut new_full_text_history = String::new();
                if self.buffer_live.len() >= VEC_MAXIMUM_SAMPLES {
                    new_full_text_history = Self::task_whisper(
                        inside_state,
                        params,
                        &mut self.buffer_live.drain(..buf_sample_len).collect(),
                    );
                }

                // send text output to GUI thread
                if !new_full_text.trim().is_empty() || !new_full_text_history.trim().is_empty() {
                    self.caption.lock().unwrap().current = new_full_text;
                    self.caption
                        .lock()
                        .unwrap()
                        .history
                        .push_str(&*new_full_text_history);
                }
            }
        }
    }

    /// Whisper will start process translate Speech Audio to Text output.
    fn task_whisper(
        whisper: &mut WhisperState,
        params: FullParams,
        buffer: &mut Vec<f32>,
    ) -> String {
        match whisper.full(params, &buffer) {
            Ok(()) => (),
            Err(e) => {
                eprintln!("Err -- running task whisper failed: {e}");
                return "".into();
            }
        };

        let mut output_text = String::new();

        for segment in whisper.as_iter() {
            let text = segment.to_string();

            if Self::is_junk(&text) {
                continue;
            }

            output_text.push_str(&text);
        }

        output_text
    }

    /// Check the Whisper's output Text, is there junk? If so, return true
    /// ```rust
    /// fn test_is_junk() {
    ///    let junk_words = String::from("bye.");
    ///    assert!(is_junk(&junk_words));
    ///    // return true
    ///}
    ///```
    ///```rust
    ///#[test]
    ///fn test_is_not_junk() {
    ///    let good_words = String::from("Hello how are you? i see, bye.");
    ///    assert!(!is_junk(&good_words));
    ///    // return false
    ///}
    ///```
    #[inline]
    fn is_junk(text: &String) -> bool {
        let text_trimmed: String = text.trim().to_lowercase();
        if text_trimmed.is_empty() {
            return true;
        }

        // common hallucinations from whisper
        const JUNK_WORDS: [&str; 14] = [
            "[blank_audio]",
            "[silence]",
            "[ silence ]",
            "(silence)",
            "[foreign language]",
            "(foreign language)",
            "you",
            "thank you.",
            "thanks for watching!",
            "bye.",
            "bye!",
            "...",
            "*Gunshot*",
            "Scrrặc",
        ];

        for phrase in JUNK_WORDS.iter() {
            if text_trimmed.contains(phrase) && text_trimmed.len() < 15 {
                return true;
            }
        }

        if text_trimmed
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(3)
            .any(|w| w[0] == w[1] && w[1] == w[2])
        {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_junk() {
        let junk_words = String::from("bye.");
        assert!(WhisperSTT::is_junk(&junk_words));
    }

    #[test]
    fn test_is_not_junk() {
        let good_words = String::from("Hello how are you? i see, bye.");
        assert!(!WhisperSTT::is_junk(&good_words));
    }

    #[test]
    fn test_is_repeat_junk() {
        let junk_word = String::from("what what what");
        assert!(WhisperSTT::is_junk(&junk_word));
    }
}
