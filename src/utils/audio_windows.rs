use std::sync::{mpsc, Arc, atomic::AtomicBool};

use wasapi::{DeviceEnumerator, Direction::Capture};

pub fn audio_worker(tx: mpsc::SyncSender<Vec<f32>>, is_ui_closed: Arc<AtomicBool>) {
    // test
    let device_enum = DeviceEnumerator::new().unwrap();

    println!("{}", device_enum.get_device_collection(&Capture).unwrap().get_nbr_devices().unwrap());
}
