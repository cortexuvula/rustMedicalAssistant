pub mod convert;
pub mod device;
pub mod state;
pub mod capture;
pub mod waveform;
pub mod playback;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Device error: {0}")]
    Device(String),
    #[error("Capture error: {0}")]
    Capture(String),
    #[error("Playback error: {0}")]
    Playback(String),
    #[error("Encoding error: {0}")]
    Encoding(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No input device available")]
    NoInputDevice,
    #[error("No output device available")]
    NoOutputDevice,
    #[error("Invalid state transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },
}

pub type AudioResult<T> = Result<T, AudioError>;
