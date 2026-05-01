pub mod ui;
pub mod ui_settings;

pub mod stt;

#[cfg(target_os = "linux")]
pub mod audio_linux;

#[cfg(target_os = "windows")]
pub mod audio_windows;

#[cfg(target_os = "macos")]
pub mod audio_macos;

#[cfg(feature = "osc")]
pub mod osc;
