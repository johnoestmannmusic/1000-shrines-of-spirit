//! Starting part-way: the same piece, joined later — exactly.

use shrine_explorer::framebuffer::Framebuffer;
use shrine_explorer::scene::Quality;
use shrine_explorer::sim::{Sim, SimConfig};
use shrine_explorer::SAMPLE_RATE;

fn audio(config: SimConfig, seconds: usize, block: usize) -> Vec<f32> {
    let mut sim = Sim::new(config);
    let mut out = vec![0f32; SAMPLE_RATE as usize * 2 * seconds];
    for chunk in out.chunks_mut(block * 2) {
        sim.render_audio(chunk);
    }
    out
}

#[test]
fn an_offset_start_plays_exactly_what_an_unbroken_run_plays_there() {
    for pace in [shrine_explorer::harmony::Pace::HalfTime, shrine_explorer::harmony::Pace::Garage] {
        let mut config = SimConfig::default();
        config.settings.pace = pace;
        let whole = audio(config.clone(), 16, 4096);
        config.settings.start_s = 9;
        let joined = audio(config.clone(), 7, 1000);
        let sr = SAMPLE_RATE as usize;
        // After the short fade-in the samples are identical, bit for bit.
        let after_fade = 2 * sr * 2;
        assert_eq!(&joined[after_fade..], &whole[9 * sr * 2 + after_fade..], "{pace:?}: the joined piece differs");
        // And it does fade in rather than start on a hit.
        assert!(joined[0] == 0.0 && joined[1] == 0.0);
    }
}

#[test]
fn the_visuals_and_weather_start_at_the_offset_too() {
    let config = SimConfig { weather: shrine_explorer::weather::WeatherKind::Storm, ..SimConfig::default() };
    let whole = Sim::new_visual(config.clone());
    let mut later = config.clone();
    later.settings.start_s = 40;
    let joined = Sim::new_visual(later);
    for t in [0.0, 2.5] {
        let (mut a, mut b) = (Framebuffer::logical(), Framebuffer::logical());
        whole.render_visual(&mut a, t + 40.0, Quality::High);
        joined.render_visual(&mut b, t, Quality::High);
        assert_eq!(a.hash(), b.hash(), "frame {t}s after the start");
    }
}

#[test]
fn the_start_offset_round_trips_and_older_recipes_start_at_zero() {
    let mut config = SimConfig::default();
    config.settings.start_s = 125;
    let words = shrine_explorer::recipe::encode(&config);
    assert_eq!(shrine_explorer::recipe::decode(&words).settings.start_s, 125);
    assert_eq!(SimConfig::default().settings.start_s, 0);
}

#[test]
fn the_whole_coupled_sound_is_the_same_however_it_is_cut_into_blocks() {
    // Storm with wildlife: splashes, drips, thunder and creature calls, all
    // scheduled from the weather while the music plays. The browser renders
    // 128 frames at a time, the native exporters 4096.
    for weather in [shrine_explorer::weather::WeatherKind::Storm, shrine_explorer::weather::WeatherKind::Dusk] {
        let mut config = SimConfig { weather, ..SimConfig::default() };
        config.settings.wildlife = true;
        let a = audio(config.clone(), 14, 4096);
        assert_eq!(a, audio(config.clone(), 14, 1000), "{weather:?}: 1000-frame blocks differ");
        assert_eq!(a, audio(config, 14, 128), "{weather:?}: 128-frame blocks differ");
    }
}
