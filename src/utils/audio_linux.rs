use std::{
    arch::x86_64::{
        _mm256_add_ps, _mm256_loadu_ps, _mm256_mul_ps, _mm256_setzero_ps, _mm256_storeu_ps,
        _mm512_add_ps, _mm512_loadu_ps, _mm512_mul_ps, _mm512_setzero_ps, _mm512_storeu_ps,
    },
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

use crate::utils::ui_settings::Settings;

// pipewire settings
const SAMPLE_RATE: u32 = 16000;
const CHANNEL: u32 = 1;

// for sample rate record every around 0.5s (24000 samples) total
const SAMPLE_CHUNK: u32 = SAMPLE_RATE / 2;

pub struct AudioWorker {
    ui_settings: Arc<Settings>,
    is_ui_closed: Arc<AtomicBool>,
}

impl AudioWorker {
    pub fn new(ui_settings: Arc<Settings>, is_ui_closed: Arc<AtomicBool>) -> Self {
        Self {
            ui_settings,
            is_ui_closed,
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

        let context = match ContextBox::new(mainloop.loop_(), None) {
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

        // check CPU if has support feature for SIMD otherwise fallback
        let selected_rms_function: unsafe fn(&[f32]) -> bool = if is_x86_feature_detected!(
            "avx512f"
        ) {
            println!("INFO -- avx512 supported detected! SIMD activated");
            Self::is_silent_rms_avx512
        } else if is_x86_feature_detected!("avx2") {
            println!("INFO -- avx2 supported detected! SIMD activated");
            Self::is_silent_rms_avx2
        } else {
            println!(
                "INFO -- No avx2 or avx512 supported detected, fallback scalar mode. SIMD deactivated"
            );
            Self::is_silent_rms
        };

        let mut audio_data_buf: Vec<f32> = Vec::with_capacity(8192);

        // clone for some stuff at near end of close bracket before move self outside.
        let mainloop_clone = mainloop.clone();
        let settings_clone = Arc::clone(&self.ui_settings);
        let is_ui_closed_clone = Arc::clone(&self.is_ui_closed);

        let _listener = match stream
            .add_local_listener::<()>()
            .process(move |stream, _data| {
                let buf = stream.dequeue_buffer();

                if let Some(mut buf_inner) = buf {
                    for data in buf_inner.datas_mut() {
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

                        audio_data_buf.extend_from_slice(samples);

                        if audio_data_buf.len() >= SAMPLE_CHUNK as usize {
                            let temp = std::mem::take(&mut audio_data_buf);

                            // reserve again to keep low capacity enough to avoid waste unused cause of auto resize.
                            audio_data_buf.reserve_exact(8192);

                            // check if it's silence samples, no need clear
                            // cause out of scope meaning temp will dropped.
                            if unsafe { !selected_rms_function(&temp) } {
                                match tx.send(temp) {
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
        let device_id = Self::get_device_id(&settings_clone.data.lock().unwrap().select_device);

        println!(
            "INFO -- Selecting device: {} (If device can't used by pipewire,it will use default: mic(if is plugged) unless you pick device otherwise does nothing)",
            settings_clone
                .data
                .lock()
                .unwrap()
                .select_device
                .clone()
                .unwrap_or("None".to_string())
        );

        println!("INFO -- Audio Thread has started!");

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
            if settings_clone.flags.should_restart_audio.load(Acquire)
                || is_ui_closed_clone.load(Acquire)
            {
                mainloop_clone.quit();
                settings_clone
                    .flags
                    .thread_exited_ready
                    .store(true, Release);
            }
        });

        timer.update_timer(
            Some(Duration::from_millis(50)),
            Some(Duration::from_millis(50)),
        );

        mainloop.run();

        println!("INFO -- Audio Thread has exited successfully!");
    }

    /// return true if audio level is too below the minimum requirement.
    /// SIMD using avx512f for sweet 512bits
    #[target_feature(enable = "avx512f")]
    unsafe fn is_silent_rms_avx512(audio_data: &[f32]) -> bool {
        unsafe {
            let (chunks, remainder) = audio_data.as_chunks::<16>();

            let mut sum = _mm512_setzero_ps();

            for chunk in chunks {
                let values = _mm512_loadu_ps(chunk.as_ptr());
                let squard = _mm512_mul_ps(values, values);
                sum = _mm512_add_ps(sum, squard);
            }

            let mut result = [0.0f32; 16];
            _mm512_storeu_ps(result.as_mut_ptr(), sum);

            let mut total = result.iter().sum::<f32>();

            for &i in remainder {
                total += i * i;
            }

            let rms = (total / audio_data.len() as f32).sqrt();

            // adjust number if filter job isn't satisfied
            rms < 0.003
        }
    }

    /// return true if audio level is too below the minimum requirement.
    /// SIMD using avx2 for sweet 256bits
    #[target_feature(enable = "avx2")]
    unsafe fn is_silent_rms_avx2(audio_data: &[f32]) -> bool {
        unsafe {
            let (chunks, remainder) = audio_data.as_chunks::<8>();

            let mut sum = _mm256_setzero_ps();

            for chunk in chunks {
                let values = _mm256_loadu_ps(chunk.as_ptr());
                let squard = _mm256_mul_ps(values, values);
                sum = _mm256_add_ps(sum, squard);
            }

            let mut result = [0.0f32; 8];
            _mm256_storeu_ps(result.as_mut_ptr(), sum);

            let mut total = result.iter().sum::<f32>();

            for &i in remainder {
                total += i * i;
            }

            let rms = (total / audio_data.len() as f32).sqrt();

            // adjust number if filter job isn't satisfied
            rms < 0.003
        }
    }

    /// For people who doesn't has feature avx512 or avx2, this is fallback version.
    /// return true if audio level is too below the minimum requirement.
    fn is_silent_rms(audio_data: &[f32]) -> bool {
        if audio_data.is_empty() {
            return true;
        }

        let sum_squares: f32 = audio_data.iter().map(|x| x * x).sum();
        let rms = (sum_squares / audio_data.len() as f32).sqrt();

        // adjust number if filter job isn't satisfied
        rms < 0.003
    }

    /// get id by find match name of device/app/other
    fn get_device_id(select_device: &Option<String>) -> Option<u32> {
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

        let device_name = select_device.clone()?;

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

                    if let Some(name) = props.get("device.description")
                        && !name.is_empty()
                        && device_name.contains(name)
                    {
                        *get_id.lock().unwrap() = Some(g.id);
                    }

                    if let Some(name) = props.get("node.description")
                        && !name.is_empty()
                        && device_name.contains(name)
                    {
                        *get_id.lock().unwrap() = Some(g.id);
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

    /// get all audio devcies/other that pipewire see.
    ///
    /// # info
    ///
    /// required taking Arc Owner! use Arc::clone() for it.
    pub fn get_devices_array(settings: Arc<Settings>) {
        let mainloop = match MainLoopRc::new(None) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Err -- Creating mainloop failed: {e}");
                return;
            }
        };

        let context = match ContextBox::new(mainloop.loop_(), None) {
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

        settings
            .data
            .lock()
            .unwrap()
            .devices
            .push("None".to_string());

        let _listener = registry
            .add_listener_local()
            .global(move |g| {
                if let Some(props) = &g.props {
                    let mut safe_array = settings.data.lock().unwrap();

                    /*if let Some(name) = props.get("application.name") {
                        if !name.is_empty() && !safe_array.contains(&name.to_string()) {
                            safe_array
                                .push(name.to_string());
                        }
                    }*/

                    if let Some(name) = props.get("device.description")
                        && !name.is_empty()
                        && !safe_array.devices.contains(&name.to_string())
                    {
                        safe_array.devices.push(name.to_string());
                    }

                    if let Some(name) = props.get("node.description")
                        && !name.is_empty()
                        && !safe_array.devices.contains(&name.to_string())
                    {
                        safe_array.devices.push(name.to_string());
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
