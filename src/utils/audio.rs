use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};

use psimple::Simple;
use pulse::stream::Direction;
use pulse::sample::{Spec, Format};

const SAMPLE_RATE: usize = 16000;

// meaning 1 sec of record
const SAMPLES_PER_CHUNK: usize = SAMPLE_RATE / 2;

// loop handling audio and send data to channel
pub fn worker(tx: mpsc::SyncSender<Vec<f32>>, is_ui_closed: Arc<AtomicBool>) {
    // init
    // set audio up
    let spec = Spec {
        format: Format::FLOAT32NE,
        channels: 1,
        rate: 16000,
    };

    // check make sure valid
    assert!(spec.is_valid());

    // run init again if detect change input for audio
    let audio_server = match Simple::new(
        None, 
        "input_audio", 
        Direction::Record,
        Some("*.monitor"),
        "Input Audio",
        &spec,
        None,
        None
    ) {
        Ok(res) => res,
        Err(e) => panic!("creating audio failed: {e}"),
    };

    let mut audio_data_bytes: [u8; 4096] = [0u8; 4096];
    let mut buffer = Vec::new();

    // start working
    // missing check logic
    while !is_ui_closed.load(Ordering::Relaxed) {
        // read from audio input
        match audio_server.read(&mut audio_data_bytes) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("audio error: {e}");
                continue;
            }
        };

        let mut chunk = bytes_to_f32(&audio_data_bytes);
        buffer.append(&mut chunk);

        // 16k per second, i guess that's how audio work.
        // keep chunk >16k instead too small data
        if buffer.len() >= SAMPLES_PER_CHUNK {
            // skip if silent
            if !is_silent(&buffer) {

                match tx.send(std::mem::take(&mut buffer)) {
                    Ok(res) => res,
                    Err(_) => break,
                }
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
    rms < 0.003
}
