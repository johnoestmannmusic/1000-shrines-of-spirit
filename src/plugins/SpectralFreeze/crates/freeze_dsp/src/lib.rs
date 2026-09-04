//! Pure-Rust DSP core for the SpectralFreeze plugin. No dependency on audio
//! hardware, a DAW host, or nih-plug - every algorithm here is a plain
//! function/struct over `Vec<f32>`/`&[f32]` that `cargo test` can verify
//! numerically. Ports the phase-vocoder freeze + cepstral formant-shift
//! algorithm from src/0006/index.html's Spectral Fusion "Freeze" feature.

pub mod analysis;
pub mod envelope;
pub mod fft;
pub mod formant;
pub mod freeze;
pub mod phase_advance;
pub mod render;
pub mod resample;
pub mod resynth;
pub mod stereo;
pub mod voice;
pub mod window;
