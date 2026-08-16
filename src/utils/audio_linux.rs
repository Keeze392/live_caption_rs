use std::{
    sync::{
        Arc, Mutex,
        atomic::{
            AtomicBool,
            Ordering::{Acquire, Release},
        },
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use nnnoiseless::DenoiseState;
use pipewire::{
    context::ContextBox,
    main_loop::MainLoopRc,
    properties::properties,
    spa::{
        param::audio::{AudioFormat, AudioInfoRaw},
        pod::Pod,
        utils::Direction,
    },
    stream::{StreamBox, StreamFlags},
};
use resampler::{ResamplerFir, SampleRate};

// pipewire settings
const SAMPLE_RATE: u32 = 48000;
const CHANNEL: u32 = 1;

// for resample
// from pipewire record settings
const INPUT_RESAMPLE_RATE: SampleRate = SampleRate::Hz48000;
// downsample to 16khz, this should stay for Whisper requirement
const OUTPUT_RESAMPLE_RATE: SampleRate = SampleRate::Hz16000;

// for sample rate record every around 0.5s (24000 samples) total
const SAMPLE_CHUNK: u32 = SAMPLE_RATE / 2;

pub struct AudioWorker {
    select_device: Arc<Mutex<Option<String>>>,
    should_restart_audio: Arc<AtomicBool>,
    is_ui_closed: Arc<AtomicBool>,
    thread_exited_ready: Arc<AtomicBool>,
}

impl AudioWorker {
    pub fn new(
        select_device: Arc<Mutex<Option<String>>>,
        should_restart_audio: Arc<AtomicBool>,
        is_ui_closed: Arc<AtomicBool>,
        thread_exited_ready: Arc<AtomicBool>,
    ) -> Self {
        Self {
            select_device: select_device,
            should_restart_audio: should_restart_audio,
            is_ui_closed: is_ui_closed,
            thread_exited_ready: thread_exited_ready,
        }
    }

    // Spawn new Thread to run background loop.
    pub fn spawn(self, tx: mpsc::SyncSender<Vec<f32>>) -> JoinHandle<()> {
        thread::spawn(move || self.run(tx))
    }

    fn run(self, tx: mpsc::SyncSender<Vec<f32>>) {
        let mainloop = match MainLoopRc::new(None) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Err -- Creating mainloop failed: {e}");
                return;
            }
        };

        let context = match ContextBox::new(&mainloop.loop_(), None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Creating context failed: {e}");
                return;
            }
        };

        let core = match context.connect(None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Creating connecting failed: {e}");
                return;
            }
        };

        let stream = match StreamBox::new(
            &core,
            "audio-capture",
            properties! {
                *pipewire::keys::MEDIA_TYPE => "Audio",
                *pipewire::keys::MEDIA_CATEGORY => "Capture",
                *pipewire::keys::MEDIA_ROLE => "Communication",
                *pipewire::keys::MEDIA_CLASS => "Stream/Input/Audio",
                *pipewire::keys::NODE_DESCRIPTION => "Live Caption Audio Capture",
                *pipewire::keys::NODE_AUTOCONNECT => "true",
            },
        ) {
            Ok(s) => s,
            Err(e) => panic!("Err -- Creating stream failed: {e}"),
        };

        let mut audio_data_buffer: Vec<f32> = Vec::with_capacity(24_150);

        let mut denoise_state = DenoiseState::new();

        let mut resampler_fir = ResamplerFir::new(
            CHANNEL as usize,
            INPUT_RESAMPLE_RATE,
            OUTPUT_RESAMPLE_RATE,
            resampler::Latency::Sample64,
            resampler::Attenuation::default(),
        );

        // clone for some stuff at near end of close bracket before move self outside.
        let mainloop_clone = mainloop.clone();
        let should_restart_audio_clone = Arc::clone(&self.should_restart_audio);
        let is_ui_closed_clone = Arc::clone(&self.is_ui_closed);
        let thread_exited_ready_clone = Arc::clone(&self.thread_exited_ready);
        let select_device_clone = Arc::clone(&self.select_device);

        let _listener = match stream
            .add_local_listener::<()>()
            .process(move |stream, _data| {
                let buffer = stream.dequeue_buffer();

                if let Some(mut inside_buffer) = buffer {
                    for data in inside_buffer.datas_mut() {
                        let chunk = data.chunk();
                        let size = chunk.size() as usize;
                        if size == 0 {
                            continue;
                        }

                        let Some(bytes) = data.data() else {
                            continue;
                        };

                        let samples = unsafe {
                            std::slice::from_raw_parts(bytes.as_ptr() as *const f32, size / 4)
                        };

                        audio_data_buffer.extend_from_slice(samples);

                        if audio_data_buffer.len() >= SAMPLE_CHUNK as usize {
                            let mut temp_buffer = std::mem::take(&mut audio_data_buffer);

                            // remove any noise background
                            temp_buffer = AudioWorker::denoise(&temp_buffer, &mut denoise_state);

                            // downsample from 48khz to 16khz
                            temp_buffer =
                                AudioWorker::downsample_audio(&temp_buffer, &mut resampler_fir);

                            // check if it's silence samples, no need clear
                            // cause out of scope meaning temp_buffer will dropped.
                            if !self.is_silent_rms(&temp_buffer) {
                                match tx.send(temp_buffer) {
                                    Ok(()) => (),
                                    Err(_) => continue,
                                }
                            }
                        }
                    }
                }
            })
            .state_changed(|_, _, old, new| println!("INFO -- Audio State: {:?} -> {:?}", old, new))
            .register()
        {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Err -- Creating add device failed: {e}");
                return;
            }
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
            Err(e) => {
                eprintln!("Err -- Pod serialize failed: {e}");
                return;
            }
        }
        .0
        .into_inner();

        let mut params = [Pod::from_bytes(&values).unwrap()];

        // get device id by find match name, this is return "option" so it may none if it fail to find.
        let device_id = Self::get_device_id(&select_device_clone);

        println!(
            "INFO -- Selecting device: {} (If device can't used by pipewire,it will use default: mic(if is plugged) unless you pick monitor otherwise does nothing)",
            select_device_clone
                .lock()
                .unwrap()
                .clone()
                .unwrap_or("None".to_string())
        );

        println!("INFO -- Audio Thread has starting!");

        match stream.connect(
            Direction::Input,
            device_id,
            StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
            &mut params,
        ) {
            Ok(()) => (),
            Err(e) => {
                eprintln!("Err -- Creating stream failed: {e}");
                return;
            }
        };

        // add timer to keep eye on check if flag has been set.
        let timer = mainloop.loop_().add_timer(move |_| {
            if should_restart_audio_clone.load(Acquire) || is_ui_closed_clone.load(Acquire) {
                mainloop_clone.quit();
                thread_exited_ready_clone.store(true, Release);
            }
        });

        timer.update_timer(
            Some(Duration::from_millis(50)),
            Some(Duration::from_millis(50)),
        );

        mainloop.run();

        println!("INFO -- Audio Thread has exited successfully!");
    }

    /// RNNoise, is it high quality to kill any background noises.
    /// # Information
    /// edited: (apparently it wasn't high quality model by default lol)
    #[inline]
    fn denoise(audio_data: &[f32], denoise_state: &mut DenoiseState) -> Vec<f32> {
        let mut output: Vec<f32> = Vec::new();
        let mut out_buf = [0.0; DenoiseState::FRAME_SIZE];
        let mut first = true;

        for chunk in audio_data.chunks_exact(DenoiseState::FRAME_SIZE) {
            denoise_state.process_frame(&mut out_buf[..], chunk);

            if !first {
                output.extend_from_slice(&out_buf[..]);
            }
            first = false;
        }

        output
    }

    /// Resample 48khz to 16khz for meet Whisper's requirement also keeping the channel at 1.
    #[inline]
    fn downsample_audio(mut audio_data: &[f32], resample_fir: &mut ResamplerFir) -> Vec<f32> {
        let mut full_output: Vec<f32> = Vec::new();

        while !audio_data.is_empty() {
            let mut output = vec![0.0f32; resample_fir.buffer_size_output()];

            let (consumed, produced) = resample_fir.resample(&audio_data, &mut output).unwrap();

            full_output.extend_from_slice(&output[..produced]);

            audio_data = &audio_data[consumed..];
        }

        full_output
    }

    /// return true if audio level is too below the minimum requirement.
    fn is_silent_rms(&self, audio_data: &[f32]) -> bool {
        if audio_data.is_empty() {
            return true;
        }

        let sum_squares: f32 = audio_data.iter().map(|x| x * x).sum();
        let rms = (sum_squares / audio_data.len() as f32).sqrt();

        // adjust number if filter job isn't satisfied
        rms < 0.003
    }

    /// get id by find match name of device/app/other
    fn get_device_id(select_device: &Arc<Mutex<Option<String>>>) -> Option<u32> {
        let mainloop = match MainLoopRc::new(None) {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Err -- Creating mainloop failed: {e}");
                return None;
            }
        };

        let context = match ContextBox::new(mainloop.loop_(), None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Creating context failed: {e}");
                return None;
            }
        };

        let core = match context.connect(None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Creating connecting failed: {e}");
                return None;
            }
        };

        let registry = match core.get_registry() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Getting registry failed: {e}");
                return None;
            }
        };

        let device_name = match select_device.lock().unwrap().clone() {
            Some(val) => val,
            None => return None,
        };

        let get_id = Arc::new(Mutex::new(Option::<u32>::None));
        let get_id_clone = Arc::clone(&get_id);

        let _listener = registry
            .add_listener_local()
            .global(move |g| {
                if let Some(props) = &g.props {
                    /*if let Some(name) = props.get("application.name") {
                        if name.contains(&*device_name) {
                            *get_id.lock().unwrap() = Some(g.id);
                        }
                    }*/

                    if let Some(name) = props.get("device.description") {
                        if !name.is_empty() && device_name.contains(name) {
                            *get_id.lock().unwrap() = Some(g.id);
                        }
                    }

                    if let Some(name) = props.get("node.description") {
                        if !name.is_empty() && device_name.contains(name) {
                            *get_id.lock().unwrap() = Some(g.id);
                        }
                    }
                }
            })
            .register();

        let mainloop_clone = mainloop.clone();

        let timer = mainloop.loop_().add_timer(move |_| mainloop_clone.quit());
        timer.update_timer(Some(Duration::from_millis(300)), None);

        mainloop.run();

        // return option<u32>
        *get_id_clone.lock().unwrap()
    }

    // get all audio devcies/other that pipewire see.
    pub fn get_devices_array(devices_array: Arc<Mutex<Vec<String>>>) {
        let mainloop = match MainLoopRc::new(None) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Err -- Creating mainloop failed: {e}");
                return;
            }
        };

        let context = match ContextBox::new(&mainloop.loop_(), None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Creating context failed: {e}");
                return;
            }
        };

        let core = match context.connect(None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Creating connecting failed: {e}");
                return;
            }
        };

        let registry = match core.get_registry() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Err -- Getting registry failed: {e}");
                return;
            }
        };

        devices_array.lock().unwrap().push("None".to_string());

        let _listener = registry
            .add_listener_local()
            .global(move |g| {
                if let Some(props) = &g.props {
                    let mut safe_array = devices_array.lock().unwrap();

                    /*if let Some(name) = props.get("application.name") {
                        if !name.is_empty() && !safe_array.contains(&name.to_string()) {
                            safe_array
                                .push(name.to_string());
                        }
                    }*/

                    if let Some(name) = props.get("device.description") {
                        if !name.is_empty() && !safe_array.contains(&name.to_string()) {
                            safe_array.push(name.to_string());
                        }
                    }

                    if let Some(name) = props.get("node.description") {
                        if !name.is_empty() && !safe_array.contains(&name.to_string()) {
                            safe_array.push(name.to_string());
                        }
                    }
                }
            })
            .register();

        let mainloop_clone = mainloop.clone();

        let timer = mainloop.loop_().add_timer(move |_| mainloop_clone.quit());
        timer.update_timer(Some(Duration::from_millis(300)), None);

        mainloop.run();
    }
}
