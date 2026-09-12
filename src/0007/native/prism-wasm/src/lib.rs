//! WASM binding around the SpectralPrism `prism_dsp` crate's freeze/fusion
//! engine, so the TypeScript renderer can call it synchronously.
//!
//! STATUS: scaffold only. It has not been compiled yet (the build environment
//! is missing the `wasm-bindgen` CLI / `wasm-pack`). Building it is tracked as
//! 0007E-PLAN-011 in `manage/KANBAN.md`; until then the renderer reports the
//! Spectral engine as unavailable and plays the plain Sampler source.
//!
//! The exported `render_fused` mirrors `lantern_core::spectral::render`:
//! Sample A (required) optionally fused with Sample B, peak-normalised, and
//! returned as interleaved-flat stereo PCM.

use js_sys::{Float32Array, Object, Reflect};
use prism_dsp::fusion::{
    effective_mode, render_fused_loop, FusionMode, FusionRenderParams,
};
use prism_dsp::render::DEFAULT_ROOT_NOTE;
use wasm_bindgen::prelude::*;

fn parse_mode(mode: &str) -> FusionMode {
    match mode {
        "mix" => FusionMode::Mix,
        "cross-synth" => FusionMode::CrossSynth,
        "convolve" => FusionMode::Convolve,
        "ring-modulate" => FusionMode::RingModulate,
        "spectral-max" => FusionMode::SpectralMax,
        "spectral-min" => FusionMode::SpectralMin,
        "cycle" => FusionMode::Cycle,
        _ => FusionMode::Off,
    }
}

fn split(flat: Vec<f32>, channels: usize) -> Result<Vec<Vec<f32>>, JsValue> {
    if channels == 0 {
        return Ok(Vec::new());
    }
    if flat.len() % channels != 0 {
        return Err(JsValue::from_str("flat PCM length is not a multiple of channel count"));
    }
    let frames = flat.len() / channels;
    let mut out = Vec::with_capacity(channels);
    for c in 0..channels {
        let mut channel = Vec::with_capacity(frames);
        for f in 0..frames {
            channel.push(flat[c * frames + f]);
        }
        out.push(channel);
    }
    Ok(out)
}

fn set(obj: &Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(obj, &JsValue::from_str(key), value)?;
    Ok(())
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render_fused(
    a_flat: Vec<f32>,
    a_channels: u32,
    b_flat: Vec<f32>,
    b_channels: u32,
    sample_rate: f32,
    freeze_point_a_pct: f32,
    volume_a_pct: f32,
    tune_a_semitones: f32,
    formant_shift_a_semitones: f32,
    mode: &str,
    freeze_point_b_pct: f32,
    formant_shift_b_semitones: f32,
    volume_b_pct: f32,
    tune_b_semitones: f32,
    mix_amount_pct: f32,
    cross_synth_amount_pct: f32,
    convolve_amount_pct: f32,
    ring_mod_amount_pct: f32,
    stereo_width_pct: f32,
    loop_length_seconds: f32,
) -> Result<Object, JsValue> {
    let a = split(a_flat, a_channels as usize)?;
    if a.is_empty() {
        return Err(JsValue::from_str("Sample A is empty"));
    }
    let b = split(b_flat, b_channels as usize)?;

    let mut fusion = FusionRenderParams {
        mode: parse_mode(mode),
        freeze_point_b_pct,
        formant_shift_b_semitones,
        volume_b_pct,
        tune_b_semitones,
        mix_amount_pct,
        cross_synth_amount_pct,
        convolve_amount_pct,
        ring_mod_amount_pct,
    };
    fusion.mode = effective_mode(fusion.mode, &b);

    let output = render_fused_loop(
        &a,
        &b,
        sample_rate,
        freeze_point_a_pct,
        volume_a_pct,
        tune_a_semitones,
        formant_shift_a_semitones,
        &fusion,
        stereo_width_pct,
        loop_length_seconds,
        DEFAULT_ROOT_NOTE,
    );

    let out_channels = output.channels.len() as u32;
    let flat: Vec<f32> = output.channels.into_iter().flatten().collect();
    let obj = Object::new();
    set(&obj, "channelCount", &JsValue::from_f64(out_channels as f64))?;
    set(&obj, "sampleRate", &JsValue::from_f64(output.sample_rate as f64))?;
    set(&obj, "data", &Float32Array::from(flat.as_slice()))?;
    Ok(obj)
}
