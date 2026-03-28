use std::io::Write;
use std::thread;
use std::sync::mpsc;

use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

use psimple::Simple;
use pulse::stream::Direction;
use pulse::sample::{Spec, Format};

fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    // vector mpsc channel
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);

    // audio_data must be 32 bits, 16K and mono for whisper absolute requirement -- done
    // from here for loop begin to become live caption -- working
    // check if sample is less than 16K, skip it(continue in loop) until it grow to more than 16k
    // as samples step? -- unsure
    // also need check if silence/quiet, skip it(continue in loop) to avoid hallucinations

    // add task for audio
    let audio_thread = thread::spawn(move || audio_worker(tx));

    // add task for model
    let stt_thread = thread::spawn(move || stt_worker(rx));

    audio_thread.join().unwrap()?;
    stt_thread.join().unwrap()?;

    Ok(())
}

// loop handling model AI speech to text
fn stt_worker(rx: mpsc::Receiver<Vec<f32>>) -> Result<(), anyhow::Error> {
    // init
    // load model
    let model_file = "../models/ggml-large-v3-turbo.bin";

    whisper_rs::install_logging_hooks();

    let ctx = WhisperContext::new_with_params(
        model_file,
        WhisperContextParameters::default()
    )?;

    // set params object up
    let params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0
    });

    let mut speech_to_text = ctx.create_state()?;

    // start working
    loop {
        speech_to_text.full(params.clone(), &rx.recv()?)?;

        for segment in speech_to_text.as_iter() {
            print!("{}", segment);
            std::io::stdout().flush()?;
        }
    }
}

// loop handling audio and send data to channel
fn audio_worker(tx: mpsc::SyncSender<Vec<f32>>) -> Result<(), anyhow::Error> {
    // init
    // set audio up
    let spec = Spec {
        format: Format::FLOAT32NE,
        channels: 1,
        rate: 16000,
    };

    // check make sure valid
    assert!(spec.is_valid());

    let audio_server = Simple::new(
        None, 
        "input_audio", 
        Direction::Record,
        None,
        "Input any audio coming",
        &spec,
        None,
        None
    )?;

    let mut audio_data_bytes: [u8; 4096] = [0u8; 4096];
    let mut buffer = Vec::new();

    // start working
    // missing check logic
    loop {
        audio_server.read(&mut audio_data_bytes)?;

        let mut chunk = bytes_to_f32(&audio_data_bytes);
        buffer.append(&mut chunk);

        // 16k per second, i guess that's how audio work.
        // keep chunk >16k instead too tiny data
        if buffer.len() >= 16000 {

            // skip if silent
            if !is_silent(&buffer) {
                tx.send(std::mem::take(&mut buffer))?;
            } else {
                buffer.clear();
            }
        }
    }
}

fn bytes_to_f32(input_data: &[u8]) -> Vec<f32> {
    input_data
        .chunks_exact(4)
        .map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn is_silent(audio_data: &[f32]) -> bool {
    if audio_data.is_empty() { return true; }

    let sum_squares: f32 = audio_data.iter().map(|x| x * x).sum();
    let rms = (sum_squares / audio_data.len() as f32).sqrt();

    // adjust number if not filter correctly
    rms < 5e-4
}
