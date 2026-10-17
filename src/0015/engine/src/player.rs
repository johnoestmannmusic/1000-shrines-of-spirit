//! A live shrine instance: the simulation, its reusable buffers and the audio
//! clock.
//!
//! This is the shared core of the C ABI. [`crate::wasm`] only wraps it in
//! `extern "C"` exports for the development page; the emitted artifact supplies
//! its own baked entry point over the same `Player`, so both share one runtime.

use crate::framebuffer::Framebuffer;
use crate::scene::{Camera, Quality};
use crate::sim::Sim;
use crate::SAMPLE_RATE;

/// A running shrine: one simulation plus the buffers the ABI hands to JS.
pub struct Player {
    sim: Sim,
    fb: Framebuffer,
    buf: Vec<f32>,
    camera: Camera,
    transparent_sky: bool,
    last_sky: ([u8; 4], [u8; 4]),
    /// The Learn page's description (built once, on first ask) and live state.
    learn: Option<String>,
    learn_state: Vec<f64>,
    weather: [f64; 6],
}

impl Player {
    pub fn new(sim: Sim) -> Self {
        let last_sky = sim.scene.sky_colors();
        Player {
            sim,
            fb: Framebuffer::logical(),
            buf: Vec::new(),
            camera: Camera::default(),
            transparent_sky: false,
            last_sky,
            learn: None,
            learn_state: vec![0.0; crate::learn::STATE_LEN],
            weather: [0.0; 6],
        }
    }

    pub fn sim(&self) -> &Sim {
        &self.sim
    }

    /// Leave the sky unfilled so a page can draw its own (and put the source
    /// waterfall in it, behind the shrine).
    pub fn set_sky_transparent(&mut self, on: bool) {
        self.transparent_sky = on;
    }

    /// The lit sky gradient endpoints from the last [`render_visual`], packed
    /// `0xRRGGBB` top in the low 24 bits and bottom in the high 24. The page
    /// draws its own sky with these, so it matches the engine exactly
    /// (including the weather's lighting).
    pub fn sky_colors(&self) -> u64 {
        let pack = |c: [u8; 4]| (c[0] as u64) << 16 | (c[1] as u64) << 8 | c[2] as u64;
        pack(self.last_sky.0) | pack(self.last_sky.1) << 24
    }

    /// Nudge the camera from the pointer, in `[-1, 1]` on each axis.
    pub fn set_camera(&mut self, yaw: f64, pitch: f64) {
        self.camera = Camera { yaw: yaw.clamp(-1.0, 1.0), pitch: pitch.clamp(-1.0, 1.0) };
    }

    /// Everything the Learn page explains about this shrine, as UTF-8 JSON.
    /// Built on first ask (it sets up the music engine once, without playing it).
    pub fn learn_json(&mut self) -> &[u8] {
        let config = &self.sim.config;
        self.learn.get_or_insert_with(|| crate::learn::json(config)).as_bytes()
    }

    /// The music's moving parts now (see `learn::STATE_LEN`); all zero for a
    /// visuals-only instance.
    pub fn learn_state(&mut self) -> &[f64] {
        if let Some(track) = self.sim.track_mut() {
            crate::learn::state(track, &mut self.learn_state);
        }
        &self.learn_state
    }

    /// The weather at `sample_clock`, for the Learn page: (wind, gust, rain,
    /// snow, fog, light), each 0..1.
    pub fn weather_now(&mut self, sample_clock: u64) -> &[f64] {
        let w = self.sim.state(sample_clock as f64 / SAMPLE_RATE as f64 + self.sim.start_offset()).weather;
        self.weather = [w.wind_strength, w.gust, w.rain, w.snow, w.fog, w.light];
        &self.weather
    }

    /// Hear one layer alone: 1.. = `learn::SOLOS`, 0 = the full mix.
    pub fn set_solo(&mut self, n: u32) {
        let solo = (n as usize).checked_sub(1).and_then(|i| crate::learn::SOLOS.get(i).copied());
        self.sim.set_solo(solo);
    }

    /// Render `frames` stereo samples and return the interleaved buffer.
    pub fn render_audio(&mut self, frames: usize) -> &[f32] {
        self.buf.resize(frames * 2, 0.0);
        self.sim.render_audio(&mut self.buf);
        &self.buf
    }

    /// Render one 300x300 RGBA frame at `sample_clock` samples.
    pub fn render_visual(&mut self, sample_clock: u64) -> &[u8] {
        let t = sample_clock as f64 / SAMPLE_RATE as f64;
        let mut state = self.sim.state(t);
        state.camera = self.camera;
        state.draw_sky = !self.transparent_sky;
        self.last_sky = self.sim.scene.render_state(&mut self.fb, &state, Quality::High);
        self.fb.as_bytes()
    }
}
