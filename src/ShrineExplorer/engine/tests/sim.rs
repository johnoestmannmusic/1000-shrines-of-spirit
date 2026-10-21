//! The coupled clock: one state must drive both the music and the picture.

use shrine_explorer::framebuffer::Framebuffer;
use shrine_explorer::plants::Biome;
use shrine_explorer::scene::{Quality, SimState};
use shrine_explorer::sim::{beat_pulse, Sim, SimConfig};
use shrine_explorer::weather::WeatherKind;
use shrine_explorer::{DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};

fn config(seed: u64, weather: WeatherKind) -> SimConfig {
    SimConfig { seed, seeds: DEFAULT_SEEDS, settings: DEFAULT_SETTINGS, biome: Biome::Moss, weather }
}

fn hash(samples: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

#[test]
fn state_is_pure_and_reproducible() {
    let a = Sim::new(config(1000, WeatherKind::Rain));
    let b = Sim::new(config(1000, WeatherKind::Rain));
    for i in 0..200 {
        let t = i as f64 * 0.37;
        assert_eq!(a.state(t), b.state(t), "t={t}");
    }
}

#[test]
fn the_visual_and_audio_share_one_weather() {
    let sim = Sim::new(config(42, WeatherKind::Storm));
    for i in 0..50 {
        let t = i as f64 * 0.9;
        assert_eq!(sim.state(t).weather, sim.scene.weather_state(t));
    }
}

#[test]
fn coupling_follows_the_weather() {
    let sim = Sim::new(config(7, WeatherKind::Wind));
    for i in 0..50 {
        let t = i as f64 * 0.5;
        let s = sim.state(t);
        assert_eq!(s.coupling.foliage_sway, s.weather.wind_strength);
        assert_eq!(s.coupling.rain_density, s.weather.rain);
        assert!((0.0..=1.0).contains(&s.coupling.glow));
    }
}

#[test]
fn the_shared_state_reaches_the_picture() {
    let sim = Sim::new(config(1000, WeatherKind::Rain));
    let render = |state: SimState| {
        let mut fb = Framebuffer::logical();
        sim.scene.render_state(&mut fb, &state, Quality::High);
        fb.hash()
    };
    let t = 3.0;
    let mut dark = sim.state(t);
    dark.coupling.glow = 0.0;
    dark.coupling.foliage_sway = 0.0;
    dark.coupling.rain_density = 0.0;
    let mut bright = sim.state(t);
    bright.coupling.glow = 1.0;
    bright.coupling.foliage_sway = 1.0;
    bright.coupling.rain_density = 1.0;
    assert_ne!(render(dark), render(bright), "the visuals ignore the shared state");
}

#[test]
fn rendering_the_visual_half_is_deterministic() {
    let sim = Sim::new(config(9, WeatherKind::Fog));
    for t in [0.0, 1.5, 7.25] {
        let mut a = Framebuffer::logical();
        let mut b = Framebuffer::logical();
        sim.render_visual(&mut a, t, Quality::High);
        sim.render_visual(&mut b, t, Quality::High);
        assert_eq!(a.hash(), b.hash(), "t={t}");
    }
}

#[test]
fn the_music_alone_still_matches_0011() {
    // The music must be bit-identical to 0011 through the Sim wrapper.
    let mut sim = Sim::new(config(1000, WeatherKind::Rain));
    let frames = 30 * SAMPLE_RATE as usize;
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_music(chunk);
    }
    assert_eq!(hash(&out), 0x9090_3170_10f2_e519, "the ported music changed");
}

#[test]
fn the_weather_bed_adds_sound_on_top() {
    let mut sim = Sim::new(config(1000, WeatherKind::Rain));
    let frames = 4 * SAMPLE_RATE as usize;
    let mut music = vec![0f32; frames * 2];
    for chunk in music.chunks_mut(4096 * 2) {
        sim.render_music(chunk);
    }
    let mut full = vec![0f32; frames * 2];
    for chunk in full.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    assert_ne!(hash(&music), hash(&full), "the weather bed is silent");
    assert!(full.iter().all(|s| s.is_finite()));
}

#[test]
fn a_storm_is_louder_than_a_clear_sky() {
    let bed_energy = |weather| {
        let mut sim = Sim::new(config(5, weather));
        let frames = 4 * SAMPLE_RATE as usize;
        let mut music = vec![0f32; frames * 2];
        for chunk in music.chunks_mut(4096 * 2) {
            sim.render_music(chunk);
        }
        let mut sim = Sim::new(config(5, weather));
        let mut full = vec![0f32; frames * 2];
        for chunk in full.chunks_mut(4096 * 2) {
            sim.render_audio(chunk);
        }
        music.iter().zip(&full).map(|(m, f)| (f - m).powi(2) as f64).sum::<f64>()
    };
    assert!(bed_energy(WeatherKind::Storm) > bed_energy(WeatherKind::Clear));
    assert!(bed_energy(WeatherKind::Wind) > bed_energy(WeatherKind::Clear));
}

#[test]
fn the_weather_bed_is_deterministic() {
    let render = || {
        let mut sim = Sim::new(config(31, WeatherKind::Wind));
        let mut out = vec![0f32; 48_000 * 2];
        for chunk in out.chunks_mut(4096 * 2) {
            sim.render_audio(chunk);
        }
        hash(&out)
    };
    assert_eq!(render(), render());
}

#[test]
fn beat_pulse_is_bounded_and_periodic() {
    for i in 0..2000 {
        let t = i as f64 * 0.01;
        let p = beat_pulse(120, t);
        assert!((0.0..=1.0).contains(&p));
    }
    // 120 bpm => a beat every 0.5 s => the value repeats each 0.5 s.
    assert!((beat_pulse(120, 0.0) - beat_pulse(120, 0.5)).abs() < 1e-12);
    assert!(beat_pulse(120, 0.0) > beat_pulse(120, 0.4));
}

#[test]
fn different_configs_give_different_shrines() {
    let a = Sim::new(SimConfig { seed: 1, seeds: DEFAULT_SEEDS, settings: DEFAULT_SETTINGS, biome: Biome::Moss, weather: WeatherKind::Rain });
    let b = Sim::new(SimConfig { seed: 2, seeds: DEFAULT_SEEDS, settings: DEFAULT_SETTINGS, biome: Biome::Moss, weather: WeatherKind::Rain });
    assert_ne!(a.scene.shrine.hash(), b.scene.shrine.hash());
}
