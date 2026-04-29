use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};

use pipewire::{context::ContextBox, main_loop::MainLoopBox, properties::properties,
    spa::{param::audio::{AudioFormat, AudioInfoRaw}, pod::Pod,
    utils::Direction}, stream::{StreamBox, StreamFlags}
};

// whisper's requirement
// F32LE recommended
const SAMPLE_RATE: u32 = 16000;
const CHANNEL: u32 = 1;

// for sample rate record every around 0.5s (8000 samples) total
const SAMPLE_CHUNK: u32 = SAMPLE_RATE / 2;

// loop handling audio and send data to channel
pub fn audio_worker(
    tx: mpsc::SyncSender<Vec<f32>>,
    is_ui_closed: Arc<AtomicBool>,
    arc_devices: Arc<Mutex<Vec<String>>>,
    arc_devices_selected: Arc<Mutex<Option<String>>>,
    //arc_stream_shared: Arc<Mutex<StreamBox>>,
) {
    let mainloop = match MainLoopBox::new(None) {
        Ok(m) => m,
        Err(e) => { eprintln!("Err -- Creating mainloop failed: {e}"); return; },
    };

    let context = match ContextBox::new(&mainloop.loop_(), None) {
        Ok(c) => c,
        Err(e) => { eprintln!("Err -- Creating context failed: {e}"); return; },
    };

    let core = match context.connect(None) {
        Ok(c) => c,
        Err(e) => { eprintln!("Err -- Creating connecting failed: {e}"); return; },
    };

    let props = properties! {
        *pipewire::keys::MEDIA_TYPE => "Audio",
        *pipewire::keys::MEDIA_CATEGORY => "Capture",
        *pipewire::keys::MEDIA_ROLE => "Capture",
        *pipewire::keys::MEDIA_CLASS => "Stream/Input/Audio",
        *pipewire::keys::NODE_DESCRIPTION => "Live Caption Audio Capture",
        *pipewire::keys::STREAM_CAPTURE_SINK => "true",
    };

    let stream = match StreamBox::new(&core, "audio-capture", props) {
        Ok(s) => s,
        Err(e) => { eprintln!("Err -- Creating stream failed: {e}"); return; },
    };

    // create own buffer for holding until minimum require to send the channel
    let mut record_buffer: Vec<f32> = Vec::new();

    let _listener = match stream
        .add_local_listener::<()>()
        .process(move |stream, _data| {
            // stop doing something to avoid segfault. This process about to disconnect.
            if is_ui_closed.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            let buffer = stream.dequeue_buffer();

            if let Some(mut inside_buffer) = buffer {
                for data in inside_buffer.datas_mut() {
                    let chunk = data.chunk();
                    let size = chunk.size() as usize;
                    if size == 0 { continue; }

                    let Some(bytes) = data.data() else {
                        continue;
                    };

                    let samples = unsafe {
                        std::slice::from_raw_parts(bytes.as_ptr() as *const f32, size / 4)
                    };

                    record_buffer.extend_from_slice(samples);

                    if record_buffer.len() >= SAMPLE_CHUNK as usize {
                        let temp_buffer = std::mem::take(&mut record_buffer);

                        if !is_silent(&temp_buffer) {
                            match tx.send(temp_buffer) {
                                Ok(()) => (),
                                Err(_) => continue,
                            }
                        }
                    }
                }
            }
        })
        .state_changed(|_, _, old, new| println!("state: {:?} -> {:?}", old, new))
        .register() {
        Ok(l) => l,
        Err(e) => { eprintln!("Err -- Creating add device failed: {e}"); return; },
    };

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_rate(SAMPLE_RATE);
    audio_info.set_channels(CHANNEL);
    audio_info.set_format(AudioFormat::F32LE);

    // this is stupid complex, too hard to understand
    let values: Vec<u8> = match pipewire::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pipewire::spa::pod::Value::Object(pipewire::spa::pod::Object {
            properties: audio_info.into(),
            type_: spa_sys::SPA_TYPE_OBJECT_Format,
            id: spa_sys::SPA_PARAM_EnumFormat,
        }),
    ) {
        Ok((a, b)) => (a, b),
        Err(e) => { eprintln!("Err -- Pod serialize failed: {e}"); return; },
    }
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).unwrap()];

    //let select_device = arc_devices_selected.lock().unwrap().clone();
    //let Some(device_id) = if select_device == 

    match stream.connect(
        Direction::Input,
        Some(51), // ID of device? might easy to pick
        StreamFlags::AUTOCONNECT
         | StreamFlags::MAP_BUFFERS
         | StreamFlags::RT_PROCESS,
        &mut params,
    ) {
        Ok(()) => (),
        Err(e) => { eprintln!("Err -- Creating stream failed: {e}"); return; },
    }

    //*arc_stream_shared.lock().unwrap() = stream;

    mainloop.run();
}

fn is_silent(audio_data: &[f32]) -> bool {
    if audio_data.is_empty() { return true; }

    let sum_squares: f32 = audio_data.iter().map(|x| x * x).sum();
    let rms = (sum_squares / audio_data.len() as f32).sqrt();

    // adjust number if filter job isn't satisfied
    rms < 0.003
}
