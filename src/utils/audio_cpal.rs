use std::sync::mpsc;

#[allow(unused)]
use cpal::{Device, HostId, OutputCallbackInfo, Sample, SampleFormat, StreamConfig, traits::{DeviceTrait, HostTrait, StreamTrait}};

//use rubato::{Fft, FixedSync, Resampler};
#[allow(unused)]
pub fn worker(tx: mpsc::SyncSender::<Vec<f32>>) {
    let host = cpal::default_host();
    
    let devices = host.input_devices().expect("no device available");

    for device in devices {

        println!("device: {}", device.description().unwrap().name());
        if device.description().unwrap().name().contains("PulseAudio Sound Server") {
            let mut supported_config_range = device
                .supported_input_configs()
                .expect("error while querying configs");

            let supported_config = supported_config_range
                .next()
                .expect("no supported")
                .with_max_sample_rate();
            
            println!("device: {}", device.description().unwrap().name());
            let stream = device.build_input_stream(
                &supported_config.config(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // handling data
                    println!("oh? {data:?}");
                },
                move |err| {
                    // err
                    eprintln!("oh no! something went wrong! {err}");
                },
                None
            ).unwrap();

            stream.play().unwrap();

            loop {
                // don't leave thread! stay park >:(
                // or stream will get dropped.
                std::thread::park();
            }
        }
    }

    /*let mut resampler = Fft::<f32>::new(
        input_sr,
        16000,
        1024,
        1,
        1,
        FixedSync::Both
    ).unwrap();

    let mut buffer: Vec<f32> = Vec::new();

    let stream = match supported_config.sample_format() {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, tx, &mut resampler, &mut buffer),
        SampleFormat::I16 => build_stream::<i16>(&device, &config, tx, &mut resampler, &mut buffer),
        SampleFormat::U16 => build_stream::<u16>(&device, &config, tx, &mut resampler, &mut buffer),
        _ => panic!("Unsupported format type"),
    };

    stream.play().unwrap();

    loop {

        std::thread::park();
    }*/
}

/*fn build_stream<T: Sample>(
    device: &Device,
    config: &StreamConfig,
    tx: mpsc::SyncSender<Vec<f32>>,
    resampler: &mut Fft<f32>,
    buffer: &mut Vec<f32>,
) -> cpal::Stream {
    device.build_output_stream(
        config, 
        move |data: &mut [T], _: OutputCallbackInfo| {
            let mono: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();

            let input = vec![mono];

            let output = resampler.process(&input).unwrap();

            let mut flat: Vec<f32> = output.into_iter().flatten().collect();

            buffer.append(&mut flat);

            while buffer.len() >= 8000 {
                let chunk = buffer.drain(..8000).collect::<Vec<f32>>();
                let _ = tx.send(chunk);
            }
        },
        |err| {
            eprintln!("stream error: {err}");
        },
        None
    ).unwrap()
}*/
