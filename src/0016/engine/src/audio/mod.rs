//! The music engine: zero-dependency DSP building blocks.
//!
//! These are the tuned DSP blocks and generators reused from 0011 (`SC001`),
//! renamed to 0012's own vocabulary (`NAMING.md`) and flattened into one
//! crate-local module tree. They are standard-library only (`HC001`, `HC008`)
//! and deterministic (`HC004`): no transcendental library calls, no
//! floating-point modes that can vary between machines.
//!
//! The modules are re-exported at the crate root, so ported DSP keeps the
//! `crate::filter`-style paths it was written with. The shared settings types
//! and the `Track` assembler live in [`track`].
//!
//! The port is complete and the 0011 golden hashes still match bit-for-bit
//! (`engine/tests/audio_golden.rs`).

pub mod ambience;
pub mod atmos;
pub mod bass;
pub mod creatures;
pub mod delay;
pub mod drone;
pub mod drone_rhythm;
pub mod drums;
pub mod fft;
pub mod filter;
pub mod fm;
pub mod garage;
pub mod glitch;
pub mod grains;
pub mod glitch_setup;
pub mod harmony;
pub mod kits;
pub mod master;
pub mod micro;
pub mod modulate;
pub mod pattern;
pub mod pips;
pub mod rain;
pub mod reverb;
pub mod stabs;
pub mod telemetry;
pub mod tempo;
pub mod texture;
pub mod thunder;
pub mod track;
pub mod weather_bed;
pub mod wind;

pub use self::track::*;
