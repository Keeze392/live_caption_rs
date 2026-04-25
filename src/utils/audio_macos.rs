// macos zzz
use std::sync::{mpsc, Arc, atomic::AtomicBool};

pub fn audio_worker(tx: mpsc::SyncSender<Vec<f32>>, is_ui_closed: Arc<AtomicBool>) {

}
