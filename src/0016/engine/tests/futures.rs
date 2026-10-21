//! The four built futures: they draw distinctly, deterministically, under
//! every weather and a turning camera, and only the roofed ones sound
//! different (the weather is heard through glass).

use shrine0016::framebuffer::Framebuffer;
use shrine0016::plants::Biome;
use shrine0016::scene::Quality;
use shrine0016::sim::{Sim, SimConfig};
use shrine0016::weather::WeatherKind;

fn config(biome: Biome, weather: WeatherKind) -> SimConfig {
    SimConfig { biome, weather, ..SimConfig::default() }
}

fn frame(config: SimConfig, t: f64) -> u64 {
    let sim = Sim::new_visual(config);
    let mut fb = Framebuffer::logical();
    sim.render_visual(&mut fb, t, Quality::High);
    fb.hash()
}

fn audio(config: SimConfig, seconds: usize) -> Vec<f32> {
    let mut sim = Sim::new(config);
    let mut out = vec![0f32; 48_000 * 2 * seconds];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    out
}

#[test]
fn every_future_draws_its_own_way_and_the_same_way_twice() {
    let mut seen = vec![(Biome::Moss, frame(config(Biome::Moss, WeatherKind::Clear), 4.0))];
    for biome in Biome::ALL.into_iter().filter(|b| b.futuristic()) {
        let h = frame(config(biome, WeatherKind::Clear), 4.0);
        assert_eq!(h, frame(config(biome, WeatherKind::Clear), 4.0), "{biome:?} is not deterministic");
        if let Some((other, _)) = seen.iter().find(|(_, o)| *o == h) {
            panic!("{biome:?} draws the same as {other:?}");
        }
        seen.push((biome, h));
    }
}

#[test]
fn every_future_renders_in_every_weather_and_turned() {
    for biome in Biome::ALL.into_iter().filter(|b| b.futuristic()) {
        for weather in WeatherKind::ALL {
            let sim = Sim::new_visual(config(biome, weather));
            let mut state = sim.state(6.0);
            let mut fb = Framebuffer::logical();
            sim.scene.render_state(&mut fb, &state, Quality::High);
            let upright = fb.hash();
            state.orbit_turns = 0.137;
            sim.scene.render_state(&mut fb, &state, Quality::High);
            assert_ne!(upright, fb.hash(), "{biome:?} {weather:?}: turning changed nothing");
        }
    }
}

#[test]
fn the_roof_is_heard_and_nothing_else_about_a_future_is() {
    // The weather bed hears the biome only through its roof (wildlife is off
    // by default), so an open future sounds exactly like the ruin, and a
    // roofed one does not.
    let ruin = audio(config(Biome::Moss, WeatherKind::Rain), 6);
    assert_eq!(ruin, audio(config(Biome::NearFuture, WeatherKind::Rain), 6), "an open future changed the sound");
    assert_ne!(ruin, audio(config(Biome::Greenhouse, WeatherKind::Rain), 6), "the greenhouse roof is not heard");
}

#[test]
fn the_wild_biomes_have_no_props_or_roof() {
    for biome in [Biome::Moss, Biome::Verdant, Biome::Fungal] {
        let sim = Sim::new_visual(config(biome, WeatherKind::Clear));
        assert!(sim.scene.props.items.is_empty() && sim.scene.roof.is_none(), "{biome:?}");
    }
    for biome in [Biome::ResearchLab, Biome::Greenhouse] {
        assert!(Sim::new_visual(config(biome, WeatherKind::Rain)).scene.roof.is_some(), "{biome:?}");
    }
}

#[test]
fn a_tile_top_is_where_the_renderer_puts_it_and_turns_with_the_camera() {
    use shrine0016::iso::{Iso, FP};
    let sim = Sim::new_visual(SimConfig::default());
    let mut state = sim.state(0.0);
    state.orbit_turns = 0.0;
    // Upright: exactly the grid projection of the column's top level.
    let mut checked = 0;
    for gy in 0..24 {
        for gx in 0..24 {
            let (x, y, level) = sim.scene.surface_point(&state, gx, gy).unwrap();
            let z = (level as i32 - 1).max(0);
            let p = Iso::grid_24().project(gx, gy, z);
            assert_eq!((x, y), (p.x as f64 / FP as f64, p.y as f64 / FP as f64), "cell {gx},{gy}");
            checked += (level > 0.0) as u32;
        }
    }
    assert!(checked > 20, "only {checked} shrine columns found");
    assert!(sim.scene.surface_point(&state, 24, 0).is_none() && sim.scene.surface_point(&state, -1, 3).is_none());
    // Turned: the same cell moves, the same way every time.
    let a = sim.scene.surface_point(&state, 12, 12).unwrap();
    state.orbit_turns = 0.1;
    let b = sim.scene.surface_point(&state, 12, 3).unwrap();
    state.orbit_turns = 0.0;
    let c = sim.scene.surface_point(&state, 12, 3).unwrap();
    assert_ne!(b, c, "turning moved nothing");
    assert_eq!(a, sim.scene.surface_point(&state, 12, 12).unwrap());
}
