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

/// A shrine: its generated scene and its music, on one clock.
pub struct Sim {
    pub config: SimConfig,
    pub scene: Scene,
    track: Option<Track>,
    weather_bed: Option<WeatherBed>,
    audio_time: f64,
}

impl Sim {
    pub fn new(config: SimConfig) -> Self {
        let scene = Scene::new(config.seed, config.biome, config.weather);
        let track = Some(Track::new(config.seeds, config.settings));
        let weather_bed = Some(WeatherBed::new(config.seed));
        Sim { config, scene, track, weather_bed, audio_time: 0.0 }
    }

    /// A visuals-only instance: same scene, generated from the same recipe,
    /// without paying to synthesise the music. Used by the page's render thread.
    pub fn new_visual(config: SimConfig) -> Self {
        let scene = Scene::new(config.seed, config.biome, config.weather);
        Sim { config, scene, track: None, weather_bed: None, audio_time: 0.0 }
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
        }
    }

    /// Draw the visual half at `t` seconds.
    pub fn render_visual(&self, fb: &mut Framebuffer, t: f64, quality: Quality) {
        self.scene.render_state(fb, &self.state(t), quality);
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
        if let Some(track) = &mut self.track {
            track.render(out);
        } else {
            out.fill(0.0);
        }
        if let Some(bed) = &mut self.weather_bed {
            let weather = &self.scene.weather;
            bed.render(out, self.audio_time, weather);
        }
        self.audio_time += out.len() as f64 / 2.0 / SAMPLE_RATE as f64;
    }
}

/// A percussive accent each beat, decaying between them: `bpm` beats per
/// minute, value in `[0, 1]`. Deterministic, and shared with the visuals.
pub fn beat_pulse(bpm: u16, t: f64) -> f64 {
    let beats = t * bpm as f64 / 60.0;
    let frac = beats - beats.floor();
    exp2(-6.0 * frac).clamp(0.0, 1.0)
}
