//! The portable recipe must round-trip exactly, and the C ABI must load with
//! the plain WebAssembly API.

use shrine_explorer::atmos::LoopTimbre;
use shrine_explorer::master::MasterFx;
use shrine_explorer::plants::Biome;
use shrine_explorer::recipe;
use shrine_explorer::sim::SimConfig;
use shrine_explorer::texture::Grid;
use shrine_explorer::track::{DroneArc, KeyChoice, LoopDesign};
use shrine_explorer::weather::WeatherKind;
use shrine_explorer::{DEFAULT_SEEDS, DEFAULT_SETTINGS};

fn custom() -> SimConfig {
    SimConfig {
        seed: 4242,
        seeds: shrine_explorer::Seeds { s1: 1, s2: 2, s3: 3, s4: 4, s5: 5, s6: 6, s7: 7 },
        settings: shrine_explorer::Settings {
            chords: 4,
            bpm: 99,
            loops: [
                LoopDesign::Events(Grid::fire()),
                LoopDesign::Sustained(LoopTimbre::Veil),
                LoopDesign::Sustained(LoopTimbre::Off),
                LoopDesign::Events(Grid::wood()),
            ],
            loop_arcs: [
                DroneArc::ALWAYS_ON,
                DroneArc { cycle_s: 120, hold_s: 40 },
                DroneArc { cycle_s: 60, hold_s: 15 },
                DroneArc::ALWAYS_ON,
            ],
            key: KeyChoice::Note(7),
            loop_transpose: -5,
            drone: DroneArc { cycle_s: 300, hold_s: 90 },
            drone_tone: 1500,
            master: MasterFx { enabled: true, tape: true, radio: true, crush: true, ..MasterFx::OFF },
            code: shrine_explorer::overlay::CodeOverlay { enabled: true, opacity: 40, speed: 80, scale: 250, angle: -25, offset: 40, plane: 2 },
            name: shrine_explorer::name::Name::from_str("Glass Garden"),
            arch: shrine_explorer::arch::ArchTransform { rot_x: 25, rot_y: 0, rot_z: -90, off_x: 3, off_y: -2, off_z: 1 },
            glitch: [
                shrine_explorer::glitch_setup::GlitchSetup {
                    fill: 20,
                    rate: 2,
                    gain: 150,
                    steps: 5,
                    ratchet: 2,
                    ..shrine_explorer::glitch_setup::GlitchSetup::DEFAULT_0
                },
                shrine_explorer::glitch_setup::GlitchSetup { enabled: false, kinds: [true, false, true, false, true, false], ..shrine_explorer::glitch_setup::GlitchSetup::DEFAULT_1 },
            ],
            camera_speed: -45,
            pace: shrine_explorer::harmony::Pace::Garage,
            garage: shrine_explorer::garage::GarageSetup {
                swing: 63,
                bass: shrine_explorer::garage::BassMode::Drone,
                stabs: shrine_explorer::garage::Stabs::Busy,
                tone: shrine_explorer::garage::StabTone::Dark,
                progression: 4,
                lift: shrine_explorer::garage::KeyLift::Every8,
            },
            map: (-2_000_000_000, 1_234_567),
            micro: shrine_explorer::micro::MicroSetup {
                pulse: shrine_explorer::micro::Pulse::Off,
                grains: shrine_explorer::micro::Grains::Dense,
                pips: shrine_explorer::micro::Pips::Busy,
                dropouts: shrine_explorer::micro::Dropouts::Many,
                chords: shrine_explorer::micro::ChordRate::EveryBar,
                shift: shrine_explorer::micro::DroneShift::Scale,
            },
            drone_rhythm: true,
            loop_rhythm: true,
            model: {
                let mut m = shrine_explorer::shrine_model::ShrineModel::EMPTY;
                m.set(2, 3, 5, 3, 1);
                m.set(15, 15, 1, 1, 0);
                m
            },
            ..DEFAULT_SETTINGS
        },
        biome: Biome::Fungal,
        weather: WeatherKind::Fog,
    }
}

#[test]
fn a_recipe_round_trips_exactly() {
    for config in [SimConfig::default(), custom()] {
        let words = recipe::encode(&config);
        let back = recipe::decode(&words);
        assert_eq!(back.seed, config.seed);
        assert_eq!(back.biome, config.biome);
        assert_eq!(back.weather, config.weather);
        assert_eq!(back.seeds, config.seeds);
        assert_eq!(back.settings.bpm, config.settings.bpm);
        assert_eq!(back.settings.chords, config.settings.chords);
        assert_eq!(back.settings.loops, config.settings.loops);
        assert_eq!(back.settings.loop_arcs, config.settings.loop_arcs);
        assert_eq!(back.settings.key, config.settings.key);
        assert_eq!(back.settings.loop_transpose, config.settings.loop_transpose);
        assert_eq!(back.settings.drone.cycle_s, config.settings.drone.cycle_s);
        assert_eq!(back.settings.drone_tone, config.settings.drone_tone);
        assert_eq!(back.settings.master, config.settings.master);
        assert_eq!(back.settings.code, config.settings.code);
        assert_eq!(back.settings.name, config.settings.name);
        assert_eq!(back.settings.arch, config.settings.arch);
        assert_eq!(back.settings.glitch, config.settings.glitch);
        assert_eq!(back.settings.model, config.settings.model);
        assert_eq!(back.settings.camera_speed, config.settings.camera_speed);
        assert_eq!(back.seeds, config.seeds);
        assert_eq!(back.settings.drone_rhythm, config.settings.drone_rhythm);
        assert_eq!(back.settings.loop_rhythm, config.settings.loop_rhythm);
        assert_eq!(back.settings.pace, config.settings.pace);
        assert_eq!(back.settings.garage, config.settings.garage);
        assert_eq!(back.settings.micro, config.settings.micro);
        assert_eq!(back.settings.map, config.settings.map);
    }
}

#[test]
fn a_version_17_recipe_gets_the_microsound_defaults() {
    let mut config = custom();
    config.settings.micro = shrine_explorer::micro::MicroSetup::DEFAULT;
    config.settings.map = (0, 0);
    let mut words = recipe::encode(&config);
    // The microsound word, then version 19's two map words.
    let micro_at = words.len() - shrine_explorer::shrine_model::WORDS - shrine_explorer::name::CAP / 4 - 3;
    words.remove(micro_at + 2);
    words.remove(micro_at + 1);
    words.remove(micro_at);
    words[0] = 17;
    assert_eq!(recipe::decode(&words).settings, config.settings);
}

#[test]
fn a_version_15_recipe_keeps_its_mode_and_gets_the_garage_defaults() {
    // Version 15 had no garage word: strip it out of a version-16 encoding
    // and the rest must decode exactly as before.
    let mut config = custom();
    config.settings.pace = shrine_explorer::harmony::Pace::Jungle;
    config.settings.garage = shrine_explorer::garage::GarageSetup::DEFAULT;
    config.settings.micro = shrine_explorer::micro::MicroSetup::DEFAULT;
    config.settings.map = (0, 0);
    let mut words = recipe::encode(&config);
    // (and version 16 had no start offset, the word after it, nor version 17
    // the microsound word after that, nor version 18 the two map words).
    let garage_at = words.len() - shrine_explorer::shrine_model::WORDS - shrine_explorer::name::CAP / 4 - 5;
    words.remove(garage_at + 4);
    words.remove(garage_at + 3);
    words.remove(garage_at + 2);
    words.remove(garage_at + 1);
    words.remove(garage_at);
    words[0] = 15;
    let back = recipe::decode(&words);
    assert_eq!(back.settings, config.settings);
    assert_eq!((back.seed, back.seeds, back.biome, back.weather), (config.seed, config.seeds, config.biome, config.weather));
}

#[test]
fn encoding_twice_is_stable() {
    let a = recipe::encode(&custom());
    let b = recipe::encode(&custom());
    assert_eq!(a, b);
    assert_eq!(a.len(), 170);
}

#[test]
fn a_version_18_recipe_is_at_home() {
    let mut config = custom();
    config.settings.map = (0, 0);
    let mut words = recipe::encode(&config);
    let map_at = words.len() - shrine_explorer::shrine_model::WORDS - shrine_explorer::name::CAP / 4 - 2;
    words.remove(map_at + 1);
    words.remove(map_at);
    words[0] = 18;
    assert_eq!(recipe::decode(&words).settings, config.settings);
}

#[test]
fn a_short_or_empty_recipe_falls_back_to_defaults() {
    assert_eq!(recipe::decode(&[]).seed, SimConfig::default().seed);
    assert_eq!(recipe::decode(&[0, 1, 2]).seed, SimConfig::default().seed);
}

#[test]
fn corrupt_words_cannot_panic() {
    // Wild indices, zero lengths, huge values: decoding must stay total.
    let words = vec![u32::MAX; 56];
    let c = recipe::decode(&words);
    let _ = (c.seeds.s1, c.settings.chords, c.settings.bpm, c.biome, c.weather);
    let words = vec![2, 0, 0, 0, 0];
    let _ = recipe::decode(&words);
}

#[test]
fn the_abi_builds_and_renders_both_halves() {
    let words = recipe::encode(&SimConfig::default());
    unsafe {
        let audio = shrine_explorer::wasm::shrine_new(words.as_ptr(), words.len() as u32, 1);
        let visual = shrine_explorer::wasm::shrine_new(words.as_ptr(), words.len() as u32, 2);
        assert!(!audio.is_null() && !visual.is_null());

        let pcm = shrine_explorer::wasm::shrine_render_audio(audio, 256);
        let samples = std::slice::from_raw_parts(pcm, 512);
        assert!(samples.iter().all(|s| s.is_finite()));

        let px = shrine_explorer::wasm::shrine_render_visual(visual, 48_000);
        let bytes = std::slice::from_raw_parts(px, 300 * 300 * 4);
        assert_eq!(bytes.len(), 360_000);

        shrine_explorer::wasm::shrine_free(audio);
        shrine_explorer::wasm::shrine_free(visual);
    }
}

#[test]
fn sky_colours_are_packed_rgb_for_the_page() {
    // The page paints its own sky from these, so the packing must match what
    // the engine actually drew: with the sky opaque, the first pixel of the
    // frame *is* the top sky colour. A BGR/RGB mix-up tinted the whole
    // background (R and B swapped).
    let words = recipe::encode(&SimConfig::default());
    unsafe {
        let v = shrine_explorer::wasm::shrine_new(words.as_ptr(), words.len() as u32, 2);
        let px = shrine_explorer::wasm::shrine_render_visual(v, 0);
        let (r, g, b) = (*px, *px.add(1), *px.add(2));
        let packed = shrine_explorer::wasm::shrine_sky_colors(v);
        let expected = ((r as u64) << 16) | ((g as u64) << 8) | b as u64;
        assert_eq!(packed & 0x00ff_ffff, expected, "sky colours are not packed 0xRRGGBB");
        shrine_explorer::wasm::shrine_free(v);
    }
}

#[test]
fn the_sky_can_be_left_transparent_for_the_page() {
    // The page draws its own sky so the source waterfall can sit behind the
    // shrine; the engine must leave those pixels clear while the shrine stays
    // opaque.
    let words = recipe::encode(&SimConfig::default());
    unsafe {
        let v = shrine_explorer::wasm::shrine_new(words.as_ptr(), words.len() as u32, 2);
        shrine_explorer::wasm::shrine_set_sky_transparent(v, 1);
        let px = shrine_explorer::wasm::shrine_render_visual(v, 0);
        let bytes = std::slice::from_raw_parts(px, 300 * 300 * 4);
        let clear = bytes.chunks_exact(4).filter(|p| p[3] == 0).count();
        let solid = bytes.chunks_exact(4).filter(|p| p[3] == 255).count();
        assert!(clear > 10_000, "sky should be transparent, got {clear} clear px");
        assert!(solid > 1_000, "the shrine should stay opaque, got {solid} solid px");
        assert_ne!(shrine_explorer::wasm::shrine_sky_colors(v), 0, "no sky colours reported");
        shrine_explorer::wasm::shrine_free(v);
    }
}

#[test]
fn a_visual_instance_renders_without_the_music() {
    // Mode 2 builds no audio engine; it must still render the scene.
    let config = SimConfig { seed: 1000, seeds: DEFAULT_SEEDS, settings: DEFAULT_SETTINGS, biome: Biome::Moss, weather: WeatherKind::Rain };
    let words = recipe::encode(&config);
    let sim = shrine_explorer::sim::Sim::new_visual(recipe::decode(&words));
    let mut a = shrine_explorer::framebuffer::Framebuffer::logical();
    let mut b = shrine_explorer::framebuffer::Framebuffer::logical();
    sim.render_visual(&mut a, 1.0, shrine_explorer::scene::Quality::High);
    sim.render_visual(&mut b, 2.0, shrine_explorer::scene::Quality::High);
    assert_ne!(a.hash(), b.hash());
}
