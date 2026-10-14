//! Wildlife: off by default, chosen by biome and seed, heard and seen from one
//! schedule, and hushed by bad weather.

use shrine0012::framebuffer::Framebuffer;
use shrine0012::plants::Biome;
use shrine0012::recipe;
use shrine0012::scene::Quality;
use shrine0012::sim::{Sim, SimConfig};
use shrine0012::weather::WeatherKind;
use shrine0012::wildlife::{residents, Wildlife};

fn audio(config: SimConfig, seconds: usize) -> Vec<f32> {
    let mut sim = Sim::new(config);
    let mut out = vec![0f32; 48_000 * 2 * seconds];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    out
}

#[test]
fn it_is_off_by_default() {
    assert!(!SimConfig::default().settings.wildlife);
    assert!(Sim::new_visual(SimConfig::default()).scene.wildlife.is_none());
}

#[test]
fn switching_it_on_is_heard() {
    let mut config = SimConfig::default();
    config.weather = WeatherKind::Clear;
    let off = audio(config.clone(), 20);
    config.settings.wildlife = true;
    let on = audio(config.clone(), 20);
    assert_ne!(off, on, "the creatures must be heard");
    assert_eq!(on, audio(config, 20), "and deterministically");
}

#[test]
fn the_biome_decides_who_lives_here_and_the_seed_picks() {
    for biome in Biome::ALL {
        let pool = residents(biome);
        for seed in 0..40 {
            let w = Wildlife::new(seed, biome);
            assert_eq!(w.residents.len(), 2);
            assert_ne!(w.residents[0].creature, w.residents[1].creature);
            assert!(w.residents.iter().all(|r| pool.contains(&r.creature)), "{biome:?}");
        }
        let distinct: std::collections::BTreeSet<String> = (0..40).map(|s| Wildlife::new(s, biome).summary()).collect();
        assert!(distinct.len() > 1, "{biome:?}: every seed gives the same creatures");
    }
    assert_ne!(residents(Biome::Moss), residents(Biome::Fungal));
}

#[test]
fn storms_and_snow_hush_them() {
    for biome in Biome::ALL {
        let w = Wildlife::new(9, biome);
        let clear = w.calls(WeatherKind::Clear, 0.0, 600.0).len();
        let storm = w.calls(WeatherKind::Storm, 0.0, 600.0).len();
        let snow = w.calls(WeatherKind::Snow, 0.0, 600.0).len();
        assert!(clear > 4 * storm.max(1) && clear > 4 * snow.max(1), "{biome:?}: {clear} {storm} {snow}");
    }
}

#[test]
fn songbirds_and_fireflies_can_be_seen() {
    // Find a seed whose Verdant wildlife includes songbirds, and one whose
    // Fungal wildlife includes fireflies; with the setting on, the frames
    // must change over a stretch of time.
    for (biome, creature) in [(Biome::Verdant, "songbirds"), (Biome::Fungal, "fireflies")] {
        let seed = (0..100).find(|s| Wildlife::new(*s, biome).summary().contains(creature)).expect("some seed has them");
        let mut config = SimConfig::default();
        config.seed = seed;
        config.biome = biome;
        config.weather = WeatherKind::Clear;
        let plain = Sim::new_visual(config.clone());
        config.settings.wildlife = true;
        let wild = Sim::new_visual(config);
        let differs = (0..120).any(|i| {
            let t = i as f64 * 0.5;
            let (mut a, mut b) = (Framebuffer::logical(), Framebuffer::logical());
            plain.render_visual(&mut a, t, Quality::High);
            wild.render_visual(&mut b, t, Quality::High);
            a != b
        });
        assert!(differs, "{creature} never seen in a minute");
    }
}

#[test]
fn the_setting_and_the_new_weathers_survive_the_recipe() {
    for weather in [WeatherKind::Snow, WeatherKind::Dusk] {
        let mut config = SimConfig::default();
        config.weather = weather;
        config.settings.wildlife = true;
        config.settings.loop_rhythm = true;
        let back = recipe::decode(&recipe::encode(&config));
        assert_eq!(back.weather, weather);
        assert!(back.settings.wildlife);
        assert!(back.settings.loop_rhythm, "the neighbouring bit is untouched");
    }
    let back = recipe::decode(&recipe::encode(&SimConfig::default()));
    assert!(!back.settings.wildlife);
}
