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
            rx,
            caption,
            is_ui_closed,
            settings,

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
                        path.to_string_lossy()
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

            if self.state.is_some() {
                // get data from channel
                let Ok(mut buffer_new) = self.rx.recv() else {
                    break;
                };

                if buffer_new.is_empty() {
                    continue;
                }

                // set params object up (struct)
                let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });

                //params.set_debug_mode(true);
                params.set_max_tokens(32);
                params.set_print_progress(false);
                params.set_print_timestamps(false);
                params.set_single_segment(true);
                params.set_translate(true);
                params.set_no_context(true);
                params.set_no_speech_thold(0.7);
                params.set_suppress_blank(true);
                params.set_token_timestamps(true);
                params.set_vad_model_path(Some("/home/keeze/Downloads/ggml-silero-v6.2.0.bin"));
                params.enable_vad(true);

                // -----------------------------------------------------------------------
                // buffer manager
                // manage the buffer, keep old samples and add new samples until
                // reach maximum size, it will push old samples chunk to buffer history
                // so it can act like word by word, wihtout need wait for every full chunk
                // -----------------------------------------------------------------------

                // add new data to buffer_live
                self.buffer_live.append(&mut buffer_new);

                // feed to model
                // live
                if self.buffer_live.len() >= VEC_MINIMUM_SAMPLES {
                    self.task_whisper(params);
                }
            }
        }
    }

    /// Whisper will start process translate Speech Audio to Text output.
    #[inline]
    fn task_whisper(&mut self, params: FullParams) {
        let Some(whisper) = self.state.as_mut() else {
            return;
        };

        if let Err(e) = whisper.full(params, &self.buffer_live) {
            eprintln!("Err -- running task whisper failed: {e}");
            return;
        };

        let buffer_len = self.buffer_live.len();
        let mut new_history = String::new();
        let mut curr = String::new();

        for segment in whisper.as_iter() {
            for i in 0..segment.n_tokens() {
                if let Some(token) = segment.get_token(i) {
                    let data = token.token_data();
                    let word = match token.to_str() {
                        Ok(val) => val,
                        Err(_) => continue,
                    };

                    if Self::is_junk(word) {
                        continue;
                    }

                    if (data.t1 * 160) >= (VEC_MAXIMUM_SAMPLES / 2) as i64
                        && buffer_len >= VEC_MAXIMUM_SAMPLES
                    {
                        new_history.push_str(word);
                    } else {
                        curr.push_str(word);
                    }
                }
            }
        }
        
        let mut caption = self.caption.lock().unwrap();

        if buffer_len >= VEC_MAXIMUM_SAMPLES {
            self.buffer_live.drain(..(VEC_MAXIMUM_SAMPLES / 2));
            
            let new_words = Self::remove_overlap(&caption.history, &new_history);

            caption.history.push_str(new_words);
        }

        caption.current = curr;
    }

    // able remove most duplicates but very unreliable
    fn remove_overlap<'a>(old: &str, new: &'a str) -> &'a str {
        let old_lower = old.to_lowercase();
        let new_lower = new.to_lowercase();

        let max_overlap = old_lower.len().min(new_lower.len());

        for len in (1..=max_overlap).rev() {
            let old_part = &old_lower[old_lower.len() - len..];

            if new_lower.starts_with(old_part) {
                return &new[len..];
            }
        }

        new
    }

    /// Check the Whisper's output Text, is there junk? If so, return true
    /// ```rust
    /// test
    /// fn test_is_junk() {
    ///    let junk_words = String::from("bye.");
    ///    assert!(is_junk(&junk_words));
    ///    // return true
    ///}
    ///```
    ///```rust
    /// test
    ///fn test_is_not_junk() {
    ///    let good_words = String::from("Hello");
    ///    assert!(!is_junk(&good_words));
    ///    // return false
    ///}
    ///```
    #[inline]
    fn is_junk(text: &str) -> bool {
        let text_trimmed: String = text.trim().to_lowercase();
        if text_trimmed.is_empty() {
            return true;
        }

        // common hallucinations from whisper
        const JUNK_WORDS: [&str; 13] = [
            "[_eot_]",
            "[_tt_200]",
            "[_tt_201]",
            "[_tt_300]",
            "[blank_audio]",
            "[silence]",
            "[ silence ]",
            "(silence)",
            "[foreign language]",
            "(foreign language)",
            "thanks for watching!",
            "*gunshot*",
            "scrrặc",
        ];

        for phrase in JUNK_WORDS.iter() {
            if text_trimmed.contains(phrase) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_junk() {
        let mut junk_word = String::from("bye.");
        assert!(WhisperSTT::is_junk(&junk_word));

        junk_word = String::from("[_EOT_]");
        assert!(WhisperSTT::is_junk(&junk_word));
    }

    #[test]
    fn test_is_not_junk() {
        let good_words = String::from("Hello");
        assert!(!WhisperSTT::is_junk(&good_words));
    }
}
