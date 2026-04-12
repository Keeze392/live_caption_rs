use cpal::{OutputCallbackInfo, SampleFormat, traits::{DeviceTrait, HostTrait}};

fn audio_cpla_worker() {
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let mut supported_config_range = device.supported_output_configs().unwrap();
    let supported_config = supported_config_range.next().unwrap().with_sample_rate(16000);
    let sample_format = supported_config.sample_format();
    let config = supported_config.into();
    //let stream = match sample_format {} for F32, I16 and U16 depends by device support

    let stream = device.build_output_stream(
        &config, 
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // handling data
        },
        move |err| {
            // handling error
            eprintln!("Error -- test audio data failed: {err}");
        },

        None
    );

}
