//! The coupled simulation clock.
//!
//! One pure state — weather, the slow instrument arcs and the visual coupling
//! derived from them — is the single source of truth for both the music and the
//! picture. The audio and the visuals can run in separate WASM instances (the
//! AudioWorklet and the page) and still agree, because both are functions of
//! absolute time: no shared memory, no message passing, no drift.
//!
//! [`Sim`] owns the generated scene and the music engine built from one
//! [`SimConfig`]. The artifact uses [`Sim::render_audio`] and
//! [`Sim::render_visual`]; the native preview can use either.

use crate::audio::weather_bed::WeatherBed;
use crate::framebuffer::Framebuffer;
use crate::math::exp2;
use crate::plants::Biome;
use crate::scene::{Camera, Instruments, Quality, Scene, SimState, VisualCoupling};
use crate::track::{Seeds, Settings, Track};
use crate::weather::WeatherKind;
use crate::wildlife::Wildlife;
use crate::SAMPLE_RATE;

/// Everything a shrine is built from, shared by audio and visuals.
#[derive(Clone, Debug)]
pub struct SimConfig {
    /// The visual/palette/weather seed.
    pub seed: u64,
    /// The music's six seeds.
    pub seeds: Seeds,
    /// The music's settings.
    pub settings: Settings,
    pub biome: Biome,
    pub weather: WeatherKind,
}

/// Which instrument drives which visual channel. Data, so the eventual editor
/// can expose it rather than hard-coding the links.
pub const COUPLING: [(&str, &str); 4] = [
    ("wind", "foliage sway and rain drift"),
    ("rain", "rain streaks and surface sheen"),
    ("drone", "altar glow"),
    ("pulse", "detail flicker"),
];

/// The camera orbit at `t` seconds, in turns. Wrapped so a long performance
/// keeps full precision, which is safe because the projection is periodic in a
/// whole turn. Built from `* / rem_euclid` only, so it is bit-identical
/// everywhere (`HC004`).
pub fn camera_orbit_turns(speed_deg_per_s: i16, t: f64) -> f64 {
    if speed_deg_per_s == 0 {
        return 0.0;
    }
    (speed_deg_per_s as f64 * t / 360.0).rem_euclid(1.0)
}

/// A shrine: its generated scene and its music, on one clock.
pub struct Sim {
    pub config: SimConfig,
    pub scene: Scene,
    track: Option<Track>,
    weather_bed: Option<WeatherBed>,
    /// Stereo frames rendered so far: an exact count, so the weather bed's
    /// clock never depends on how the audio is cut into blocks.
    audio_samples: u64,
    /// Samples output since a mid-piece start, for its fade-in.
    since_start: u64,
}

/// A start part-way through fades in over this long, so it never begins
/// on the middle of a hit.
const START_FADE_S: f64 = 1.5;

impl Sim {
    pub fn new(config: SimConfig) -> Self {
        let mut scene = Scene::new(config.seed, config.biome, config.weather);
        scene.author(&config.settings.model);
        let track = Some(Track::new(config.seeds, config.settings));
        // Which cells hold a column, so a splash there is heard on stone.
        let hard = (0..crate::GRID_H as i32)
            .flat_map(|gy| (0..crate::GRID_W as i32).map(move |gx| (gx, gy)))
            .map(|(gx, gy)| scene.column_at(gx as f64, gy as f64).is_some())
            .collect();
        let wildlife = config.settings.wildlife.then(|| Wildlife::new(config.seed, config.biome));
        scene.wildlife = wildlife.clone();
        let weather_bed = Some(WeatherBed::new(config.seed, hard, wildlife).roofed(config.biome.roofed()));
        let mut sim = Sim { config, scene, track, weather_bed, audio_samples: 0, since_start: 0 };
        sim.fast_forward();
        sim
    }

    /// Where the piece starts, in seconds.
    pub fn start_offset(&self) -> f64 {
        self.config.settings.start_s.min(crate::track::MAX_START_S) as f64
    }

    /// Render, and throw away, everything before the start offset. Rendering
    /// does not depend on how it is cut into blocks, so what follows is
    /// exactly what an unbroken run would play there.
    fn fast_forward(&mut self) {
        let frames = (self.start_offset() * SAMPLE_RATE as f64) as usize;
        let mut scratch = vec![0f32; 4096 * 2];
        let mut left = frames;
        while left > 0 {
            let n = left.min(4096);
            self.render_raw(&mut scratch[..n * 2]);
            left -= n;
        }
    }

    /// A visuals-only instance: same scene, generated from the same recipe,
    /// without paying to synthesise the music. Used by the page's render thread.
    pub fn new_visual(config: SimConfig) -> Self {
        let mut scene = Scene::new(config.seed, config.biome, config.weather);
        scene.author(&config.settings.model);
        scene.wildlife = config.settings.wildlife.then(|| Wildlife::new(config.seed, config.biome));
        Sim { config, scene, track: None, weather_bed: None, audio_samples: 0, since_start: 0 }
    }

    /// The shared state at `t` seconds. Pure: the same `t` always yields the
    /// same state, so two instances stay in step.
    pub fn state(&self, t: f64) -> SimState {
        let weather = self.scene.weather_state(t);
        let sample = (t.max(0.0) * SAMPLE_RATE as f64) as u64;
        let drone = self.config.settings.drone.gain(sample);
        let loops = self.config.settings.loop_arcs.map(|a| a.gain(sample));
        let pulse = beat_pulse(self.config.settings.bpm, t);

        SimState {
            time: t,
            weather,
            coupling: VisualCoupling {
                foliage_sway: weather.wind_strength,
                rain_density: weather.rain,
                glow: drone,
            },
            instruments: Instruments { drone, loops, pulse },
            camera: Camera::default(),
            draw_sky: true,
            arch: self.config.settings.arch,
            orbit_turns: camera_orbit_turns(self.config.settings.camera_speed, t),
        }
    }

    /// Draw the visual half `t` seconds after the piece starts (so at
    /// `start offset + t` into it, in step with the audio).
    pub fn render_visual(&self, fb: &mut Framebuffer, t: f64, quality: Quality) {
        self.scene.render_state(fb, &self.state(t + self.start_offset()), quality);
    }

    /// Render the ported 0011 music alone. Bit-identical to 0011; used to prove
    /// the weather bed does not disturb it.
    pub fn render_music(&mut self, out: &mut [f32]) {
        if let Some(track) = &mut self.track {
            track.render(out);
        } else {
            out.fill(0.0);
        }
    }

    /// Render the full shrine: the music, with wind and rain mixed on top from
    /// the same weather state the visuals see.
    pub fn render_audio(&mut self, out: &mut [f32]) {
        self.render_raw(out);
        if self.config.settings.start_s > 0 {
            let fade = (START_FADE_S * SAMPLE_RATE as f64) as u64;
            for frame in out.chunks_mut(2) {
                if self.since_start >= fade {
                    break;
                }
                let x = self.since_start as f64 / fade as f64;
                for s in frame.iter_mut() {
                    *s *= (x * x) as f32;
                }
                self.since_start += 1;
            }
        }
    }

    fn render_raw(&mut self, out: &mut [f32]) {
        if let Some(track) = &mut self.track {
            track.render(out);
        } else {
            out.fill(0.0);
        }
        if let Some(bed) = &mut self.weather_bed {
            let weather = &self.scene.weather;
            bed.render_from(self.audio_samples, out, weather);
        }
        self.audio_samples += out.len() as u64 / 2;
    }
}

/// A percussive accent each beat, decaying between them: `bpm` beats per
/// minute, value in `[0, 1]`. Deterministic, and shared with the visuals.
pub fn beat_pulse(bpm: u16, t: f64) -> f64 {
    let beats = t * bpm as f64 / 60.0;
    let frac = beats - beats.floor();
    exp2(-6.0 * frac).clamp(0.0, 1.0)
}
