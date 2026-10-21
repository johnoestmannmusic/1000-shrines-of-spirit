//! Weather must be a bounded, pure function of time, and each kind must read
//! distinctly.

use shrine_explorer::audio::rain::Rain;
use shrine_explorer::audio::thunder::Thunder;
use shrine_explorer::audio::weather_bed::WeatherBed;
use shrine_explorer::framebuffer::Framebuffer;
use shrine_explorer::scene::Quality;
use shrine_explorer::sim::{Sim, SimConfig};
use shrine_explorer::weather::{EventKind, Weather, WeatherKind};

#[test]
fn state_is_pure_and_reproducible() {
    for kind in WeatherKind::ALL {
        let a = Weather::new(kind, 1000);
        let b = Weather::new(kind, 1000);
        for i in 0..500 {
            let t = i as f64 * 0.37;
            assert_eq!(a.state(t), b.state(t), "{kind:?} at t={t}");
        }
    }
}

#[test]
fn every_value_is_bounded() {
    for kind in WeatherKind::ALL {
        let w = Weather::new(kind, 7);
        for i in 0..2000 {
            let s = w.state(i as f64 * 1.13);
            for (name, v) in [
                ("wind", s.wind_strength),
                ("gust", s.gust),
                ("rain", s.rain),
                ("fog", s.fog),
                ("light", s.light),
                ("snow", s.snow),
                ("cover", s.cover),
                ("embers", s.embers),
                ("motes", s.motes),
                ("leaves", s.leaves),
                ("cloud", s.cloud),
                ("drips", s.drips),
                ("flash", s.flash),
            ] {
                assert!((0.0..=1.0).contains(&v), "{kind:?} {name}={v}");
            }
            assert!((0.0..1.0).contains(&s.wind_dir));
        }
    }
}

#[test]
fn rain_swells_and_eases_over_time() {
    let w = Weather::new(WeatherKind::Rain, 3);
    let samples: Vec<f64> = (0..4000).map(|i| w.state(i as f64 * 0.25).rain).collect();
    let lo = samples.iter().cloned().fold(f64::MAX, f64::min);
    let hi = samples.iter().cloned().fold(f64::MIN, f64::max);
    assert!(hi - lo > 0.15, "rain barely moves: {lo:.2}..{hi:.2}");
}

#[test]
fn calm_kinds_are_calmer_than_wild_ones() {
    let at = |kind, t| Weather::new(kind, 1).state(t);
    let clear = at(WeatherKind::Clear, 5.0);
    let storm = at(WeatherKind::Storm, 5.0);
    assert!(storm.rain > clear.rain);
    assert!(storm.wind_strength > clear.wind_strength);
    assert!(clear.light > storm.light);

    let fog = at(WeatherKind::Fog, 5.0);
    assert!(fog.fog > clear.fog);
}

#[test]
fn the_kind_never_changes() {
    let w = Weather::new(WeatherKind::Wind, 99);
    for i in 0..100 {
        let _ = w.state(i as f64 * 3.0);
        assert_eq!(w.kind, WeatherKind::Wind);
    }
}

#[test]
fn the_same_kind_differs_between_shrines() {
    let a = Weather::new(WeatherKind::Rain, 1);
    let b = Weather::new(WeatherKind::Rain, 2);
    let differ = (0..200).any(|i| {
        let t = i as f64;
        a.state(t).rain != b.state(t).rain
    });
    assert!(differ, "two shrines with the same weather cycle identically");
}

/// The bed alone for `kind`, `seconds` long.
fn bed(kind: WeatherKind, seconds: usize) -> (Sim, Vec<f32>) {
    let mut config = SimConfig::default();
    config.weather = kind;
    let sim = Sim::new_visual(config.clone());
    let mut bed = WeatherBed::new(config.seed, vec![false; 24 * 24], None);
    let mut out = vec![0f32; 48_000 * 2 * seconds];
    let mut t = 0.0;
    for chunk in out.chunks_mut(4096 * 2) {
        bed.render(chunk, t, &sim.scene.weather);
        t += chunk.len() as f64 / 2.0 / 48_000.0;
    }
    (sim, out)
}

fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma) * (x - ma)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb) * (y - mb)).sum();
    cov / (va * vb).sqrt().max(1e-12)
}

#[test]
fn the_bed_rises_and_falls_with_what_is_drawn() {
    // The scene draws its rain, snow and leaves from the same levels the bed
    // plays from, so the bed's loudness must follow them: measured in
    // two-second windows over a minute and a half.
    for (kind, level) in [
        (WeatherKind::Rain, (|s: &shrine_explorer::weather::WeatherState| s.rain) as fn(&_) -> f64),
        (WeatherKind::Storm, |s| s.rain),
        (WeatherKind::Snow, |s| s.snow),
        (WeatherKind::Wind, |s| s.wind_strength),
    ] {
        let (sim, out) = bed(kind, 90);
        let rms: Vec<f64> = out
            .chunks(2 * 2 * 48_000)
            .map(|c| (c.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>() / c.len() as f64).sqrt())
            .collect();
        let levels: Vec<f64> = (0..rms.len()).map(|i| level(&sim.scene.weather.state(2.0 * i as f64 + 1.0))).collect();
        let r = correlation(&rms, &levels);
        assert!(r > 0.8, "{kind:?}: the bed follows the drawn intensity only {r:.2}");
    }
}

#[test]
fn no_rain_means_no_hiss() {
    // The old wash had a floor, so a clear sky still hissed.
    let mut rain = Rain::new(5);
    for _ in 0..10_000 {
        assert_eq!(rain.next(0.0), (0.0, 0.0));
    }
}

#[test]
fn thunder_comes_after_its_flash() {
    let w = Weather::new(WeatherKind::Storm, 1000);
    let strikes = w.events(EventKind::Strike, 0.0, 300.0);
    assert!(strikes.len() > 10, "a storm strikes now and then: {}", strikes.len());
    for e in &strikes {
        let (_, distance) = e.strike();
        let delay = e.thunder_at() - e.at;
        assert!((0.3..=3.0).contains(&delay), "delay {delay}");
        assert!((delay - (0.3 + 2.7 * distance)).abs() < 1e-9, "closer strikes are heard sooner");
        assert!(w.state(e.at + 0.01).flash > 0.0, "the sky flashes at the strike");
    }
    // A roll is silent until its thunder arrives, then rumbles.
    let mut thunder = Thunder::default();
    thunder.add(4_800, 1.0, 0.2, 0.0, 7);
    let before: f64 = (0..4_800).map(|_| thunder.next().0.abs()).sum();
    let after: f64 = (0..48_000).map(|_| thunder.next().0.abs()).sum();
    assert_eq!(before, 0.0);
    assert!(after > 1.0, "the thunder is heard: {after}");
}

#[test]
fn calm_weathers_have_no_lightning() {
    for kind in WeatherKind::ALL {
        let w = Weather::new(kind, 3);
        let strikes = w.events(EventKind::Strike, 0.0, 600.0);
        assert_eq!(strikes.is_empty(), kind != WeatherKind::Storm, "{kind:?}");
    }
}

#[test]
fn events_do_not_depend_on_how_time_is_cut_up() {
    // The audio asks for one block at a time and the page for one frame at a
    // time; both must find exactly the same events.
    for kind in [EventKind::Splash, EventKind::Drip, EventKind::Spark, EventKind::Strike, EventKind::Butterfly] {
        for weather in [WeatherKind::Rain, WeatherKind::Fog, WeatherKind::Dusk, WeatherKind::Storm] {
            let w = Weather::new(weather, 11);
            let mut whole: Vec<u64> = w.events(kind, 0.0, 20.0).iter().map(|e| e.at.to_bits()).collect();
            let mut parts: Vec<u64> = (0..200)
                .flat_map(|i| w.events(kind, i as f64 * 0.1, (i + 1) as f64 * 0.1))
                .map(|e| e.at.to_bits())
                .collect();
            whole.sort();
            parts.sort();
            assert_eq!(whole, parts, "{kind:?} in {weather:?}");
        }
    }
}

#[test]
fn heavier_rain_splashes_more() {
    let w = Weather::new(WeatherKind::Rain, 4);
    let mut light = (0.0, 0usize);
    let mut heavy = (0.0, 0usize);
    for i in 0..120 {
        let t = i as f64;
        let n = w.events(EventKind::Splash, t, t + 1.0).len();
        let rain = w.state(t + 0.5).rain;
        if rain < 0.35 {
            light = (light.0 + rain, light.1 + n);
        } else if rain > 0.55 {
            heavy = (heavy.0 + rain, heavy.1 + n);
        }
    }
    assert!(heavy.1 > light.1, "heavy rain {heavy:?} vs light {light:?}");
    assert!(Weather::new(WeatherKind::Clear, 4).events(EventKind::Splash, 0.0, 60.0).is_empty());
}

#[test]
fn every_kind_renders_distinctly() {
    let mut seen: Vec<(WeatherKind, u64)> = Vec::new();
    for kind in WeatherKind::ALL {
        let mut config = SimConfig::default();
        config.weather = kind;
        let sim = Sim::new_visual(config);
        let mut fb = Framebuffer::logical();
        sim.render_visual(&mut fb, 7.0, Quality::High);
        let h = fb.hash();
        if let Some((other, _)) = seen.iter().find(|(_, o)| *o == h) {
            panic!("{kind:?} renders the same as {other:?}");
        }
        seen.push((kind, h));
    }
}

#[test]
fn weather_effects_reach_a_page_drawn_sky() {
    // With the page painting the sky, the frame is transparent there; clouds
    // and fog must still arrive, as partly transparent pixels for the page to
    // composite.
    for kind in [WeatherKind::Fog, WeatherKind::Storm] {
        let mut config = SimConfig::default();
        config.weather = kind;
        let sim = Sim::new_visual(config);
        let mut state = sim.state(3.0);
        state.draw_sky = false;
        let mut fb = Framebuffer::logical();
        sim.scene.render_state(&mut fb, &state, Quality::High);
        let partial = fb.as_bytes().chunks_exact(4).filter(|p| p[3] > 0 && p[3] < 255).count();
        assert!(partial > 2_000, "{kind:?}: only {partial} composited sky pixels");
    }
}

#[test]
fn butterflies_are_a_rare_sight_in_clear_weather() {
    // About one every two minutes, and only in clear weather.
    for seed in [1, 2, 3] {
        let clear = Weather::new(WeatherKind::Clear, seed);
        let n = clear.events(EventKind::Butterfly, 0.0, 1_200.0).len();
        assert!((9..=11).contains(&n), "seed {seed}: {n} butterflies in 20 minutes");
        // Never two at once.
        let mut at: Vec<f64> = clear.events(EventKind::Butterfly, 0.0, 1_200.0).iter().map(|e| e.at).collect();
        at.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(at.windows(2).all(|p| p[1] - p[0] > shrine_explorer::weather_fx::BUTTERFLY_S));
    }
    for kind in WeatherKind::ALL.into_iter().filter(|k| *k != WeatherKind::Clear) {
        assert!(Weather::new(kind, 1).events(EventKind::Butterfly, 0.0, 1_200.0).is_empty(), "{kind:?}");
    }
    // Clear no longer blows leaves about: those were what read as butterflies.
    assert_eq!(Weather::new(WeatherKind::Clear, 1).state(30.0).leaves, 0.0);
}

#[test]
fn the_wind_is_dry() {
    // No rain at all in Wind: none drawn, no splashes, no taps, no wash.
    for seed in [1u64, 1003, 4242] {
        let w = Weather::new(WeatherKind::Wind, seed);
        for i in 0..400 {
            assert_eq!(w.state(i as f64 * 1.7).rain, 0.0, "seed {seed}: rain in the wind");
        }
        assert!(w.events(EventKind::Splash, 0.0, 600.0).is_empty(), "seed {seed}: splashes in the wind");
    }
}

#[test]
fn the_wind_turns_round_slowly_and_particles_follow_it_exactly() {
    use shrine_explorer::weather::{Weather, WeatherKind};
    for seed in 0..16u64 {
        for kind in WeatherKind::ALL {
            let w = Weather::new(kind, seed);
            // Each way for at least 10 s (in fact half the drift period, 30 s or more).
            // (The wind was already blowing before t = 0, so the first run is
            // only timed from its first turn.)
            let (mut last_sign, mut since, mut seen_turn) = (0.0f64, 0.0f64, false);
            let mut t = 0.0;
            while t < 600.0 {
                let s = w.state(t);
                let sign = if s.wind_x > 1e-9 { 1.0 } else if s.wind_x < -1e-9 { -1.0 } else { last_sign };
                if sign != last_sign && last_sign != 0.0 {
                    if seen_turn {
                        assert!(since >= 10.0, "{kind:?} seed {seed}: turned round after {since} s at t = {t}");
                    }
                    seen_turn = true;
                    since = 0.0;
                }
                last_sign = sign;
                since += 0.25;
                t += 0.25;
            }
            // The drift is the wind's integral: its rate of change is the wind.
            for t in [3.0, 47.5, 211.0] {
                let (a, b) = (w.state(t - 0.01), w.state(t + 0.01));
                let rate = (b.drift - a.drift) / 0.02;
                assert!((rate - w.state(t).wind_x).abs() < 1e-3, "{kind:?}: drift does not follow the wind");
            }
        }
    }
}
