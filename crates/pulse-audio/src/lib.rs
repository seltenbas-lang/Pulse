//! Playback for Pulse: decode (symphonia), the original's EQ chain (audio-engine.js), volume,
//! playback rate, and output (cpal: AAudio on Android, ALSA on Linux).
pub mod dsp;
pub mod player;

pub use player::{Event, Growing, Player};
