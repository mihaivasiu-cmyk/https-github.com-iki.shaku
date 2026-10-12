//! Iki sound kernel — everything that is not presentation: the DSP, the audio
//! engine and its cpal I/O, sample loading, looper, percussion, MIDI parsing
//! and the UI-agnostic [`controller::Controller`] that turns gestures into
//! engine commands. Shared by the desktop (egui) and Android frontends.

pub mod audio;
pub mod controller;
pub mod dsp;
pub mod engine;
pub mod params;
pub mod shaku;
pub mod spectrum;

pub use controller::{Controller, Key};
