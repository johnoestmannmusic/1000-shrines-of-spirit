//! Scene assembly, weather shading and the effects budget.

use shrine0012::budget::Budget;
use shrine0012::framebuffer::Framebuffer;
use shrine0012::plants::{Biome, Plants};
use shrine0012::scene::{Camera, Quality, Scene, SimState};
use shrine0012::shrine::Shrine;
use shrine0012::weather::WeatherKind;
use shrine0012::{LOGICAL_H, LOGICAL_W};

fn scene(seed: u64) -> Scene {
    Scene::new(seed, Biome::Moss, WeatherKind::Rain)
}

#[test]
fn a_frame_is_deterministic() {
    let s = scene(1000);
    let render = |t: f64| {
        let mut fb = Framebuffer::logical();
        s.render(&mut fb, t, Quality::High);
        fb.hash()
    };
    assert_eq!(render(0.0), render(0.0));
    assert_eq!(render(3.5), render(3.5));
    assert_ne!(render(0.0), render(3.5), "the scene is not animating");
}

#[test]
fn the_buffer_never_changes_size() {
    let s = scene(1000);
    for quality in [Quality::High, Quality::Medium, Quality::Low] {
        let mut fb = Framebuffer::logical();
        s.render(&mut fb, 1.0, quality);
        assert_eq!((fb.width(), fb.height()), (LOGICAL_W, LOGICAL_H));
    }
}

#[test]
fn lower_quality_draws_fewer_effects() {
    let s = scene(7);
    let hash = |q| {
        let mut fb = Framebuffer::logical();
        s.render(&mut fb, 12.0, q);
        fb.hash()
    };
    assert_ne!(hash(Quality::High), hash(Quality::Low));
}

#[test]
fn weather_lights_the_scene() {
    let clear = Scene::new(3, Biome::Verdant, WeatherKind::Clear);
    let storm = Scene::new(3, Biome::Verdant, WeatherKind::Storm);
    let brightness = |s: &Scene| {
        let mut fb = Framebuffer::logical();
        s.render(&mut fb, 4.0, Quality::High);
        let sum: u64 = fb.as_bytes().chunks_exact(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
        sum
    };
    assert!(brightness(&clear) > brightness(&storm), "clear should be brighter than storm");
}

#[test]
fn a_page_drawn_sky_does_not_keep_the_previous_frame() {
    // The page draws its own sky, so the engine leaves it unfilled. The buffer
    // is reused between frames, so that path must still clear: otherwise the
    // previous frame's shrine lingers and the sky ghosts as the camera moves.
    let s = scene(1000);
    let mut used = Framebuffer::logical();

    let mut first = SimState::from_weather(0.0, s.weather_state(0.0));
    first.draw_sky = true;
    s.render_state(&mut used, &first, Quality::High);

    let mut moved = SimState::from_weather(0.5, s.weather_state(0.5));
    moved.draw_sky = false;
    moved.camera = Camera { yaw: 1.0, pitch: 0.0 };
    s.render_state(&mut used, &moved, Quality::High);

    // The same frame drawn on a clean buffer must be identical.
    let mut fresh = Framebuffer::logical();
    s.render_state(&mut fresh, &moved, Quality::High);
    assert_eq!(used.hash(), fresh.hash(), "the previous frame bled through");
}

#[test]
fn scene_data_is_reproducible() {
    let a = scene(55);
    let b = scene(55);
    assert_eq!(a.shrine.hash(), b.shrine.hash());
    assert_eq!(a.plants.hash(), b.plants.hash());
    assert_eq!(a.palette, b.palette);
    assert_eq!(a.weather, b.weather);
}

#[test]
fn plants_sway_with_wind_over_time() {
    let wind = Scene::new(21, Biome::Verdant, WeatherKind::Wind);
    let mut previous = 0u64;
    let mut changed = 0;
    for i in 0..12 {
        let mut fb = Framebuffer::logical();
        wind.render(&mut fb, i as f64 * 0.5, Quality::High);
        let h = fb.hash();
        if i > 0 && h != previous {
            changed += 1;
        }
        previous = h;
    }
    assert!(changed >= 10, "windy scene barely changes: {changed}/11");
}

#[test]
fn budget_steps_down_when_slow_and_back_up_when_fast() {
    let mut budget = Budget::new();
    assert_eq!(budget.quality(), Quality::High);
    for _ in 0..8 {
        budget.observe(30.0);
    }
    assert_eq!(budget.quality(), Quality::Medium);
    for _ in 0..8 {
        budget.observe(30.0);
    }
    assert_eq!(budget.quality(), Quality::Low);
    for _ in 0..8 {
        budget.observe(30.0);
    }
    assert_eq!(budget.quality(), Quality::Low, "cannot go below Low");
    for _ in 0..240 {
        budget.observe(5.0);
    }
    assert_eq!(budget.quality(), Quality::Medium);
    for _ in 0..240 {
        budget.observe(5.0);
    }
    assert_eq!(budget.quality(), Quality::High);
}

#[test]
fn budget_ignores_brief_spikes() {
    let mut budget = Budget::new();
    for _ in 0..30 {
        budget.observe(60.0);
        budget.observe(8.0);
    }
    assert_eq!(budget.quality(), Quality::High);
}

#[test]
fn scene_uses_the_generated_shrine_and_plants() {
    let s = scene(1000);
    assert_eq!(s.shrine.hash(), Shrine::generate(1000).hash());
    assert_eq!(s.plants.hash(), Plants::generate(1000, &s.shrine, Biome::Moss).hash());
}
