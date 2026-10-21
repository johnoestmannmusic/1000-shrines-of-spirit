//! The ported music engine must sound exactly like 0011, bit-for-bit.
//!
//! These are 0011's canonical golden hashes, re-run against 0012's `Track`.
//! If any of them changes, the sound changed — only update them deliberately,
//! as a new version of the piece.

use shrine0016::harmony::{Pace, Scale};
use shrine0016::{Seeds, Settings, Track, DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};

/// FNV-1a over the raw f32 bit patterns, the same hash 0011 used.
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

fn render_with(seeds: Seeds, settings: Settings, frames: usize, block: usize) -> Vec<f32> {
    let mut track = Track::new(seeds, settings);
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(block * 2) {
        track.render(chunk);
    }
    out
}

fn render(seeds: Seeds, frames: usize, block: usize) -> Vec<f32> {
    render_with(seeds, DEFAULT_SETTINGS, frames, block)
}

const SECONDS: usize = 30;

/// 0011's canonical track. If this changes, the music changed.
const GOLDEN_HASH: u64 = 0x9090_3170_10f2_e519;
/// 0010's canonical sound: the default settings without atmosphere loops.
const NO_LOOPS_GOLDEN_HASH: u64 = 0x3231_a5d5_29d9_a713;
/// The sound from before "drum space" existed (centred = mono drums).
const CENTRED_GOLDEN_HASH: u64 = 0x2c58_bac5_9719_fe83;
/// The phase-2 features: both grids, a fixed root note and a negative transpose.
const NEW_FEATURES_GOLDEN_HASH: u64 = 0xa088_09da_8bef_0296;
/// The phase-3 features: four layers, per-layer arcs and the drone character filter.
const PHASE3_GOLDEN_HASH: u64 = 0xc973_b160_a58e_fc49;

const OLD_DEFAULTS: Settings = Settings { loops: shrine0016::NO_LOOPS, ..DEFAULT_SETTINGS };

#[test]
fn golden_hash_matches_0011() {
    let out = render(DEFAULT_SEEDS, SECONDS * SAMPLE_RATE as usize, 4096);
    assert_eq!(hash(&out), GOLDEN_HASH, "0011's canonical sound changed");
}

#[test]
fn without_loops_sounds_exactly_like_0010() {
    let h = hash(&render_with(DEFAULT_SEEDS, OLD_DEFAULTS, SECONDS * SAMPLE_RATE as usize, 4096));
    assert_eq!(h, NO_LOOPS_GOLDEN_HASH, "0010's sound changed");
}

#[test]
fn centred_drums_sound_exactly_as_before() {
    let centred = Settings { space: shrine0016::kits::DrumSpace::Centred, ..OLD_DEFAULTS };
    let h = hash(&render_with(DEFAULT_SEEDS, centred, SECONDS * SAMPLE_RATE as usize, 4096));
    assert_eq!(h, CENTRED_GOLDEN_HASH, "centred drums changed");
}

#[test]
fn new_features_golden_hash() {
    use shrine0016::texture::Grid;
    use shrine0016::{KeyChoice, LoopDesign};
    let settings = Settings {
        loops: [LoopDesign::Events(Grid::fire()), LoopDesign::Events(Grid::water()), LoopDesign::OFF, LoopDesign::OFF],
        key: KeyChoice::Note(5),
        loop_transpose: -3,
        ..DEFAULT_SETTINGS
    };
    let h = hash(&render_with(DEFAULT_SEEDS, settings, SECONDS * SAMPLE_RATE as usize, 4096));
    assert_eq!(h, NEW_FEATURES_GOLDEN_HASH, "the phase-2 features changed");
}

#[test]
fn phase3_golden_hash() {
    use shrine0016::atmos::LoopTimbre;
    use shrine0016::texture::Grid;
    use shrine0016::{DroneArc, KeyChoice, LoopDesign};
    let settings = Settings {
        loops: [
            LoopDesign::Events(Grid::fire()),
            LoopDesign::Events(Grid::stones()),
            LoopDesign::Sustained(LoopTimbre::Hush),
            LoopDesign::Sustained(LoopTimbre::Veil),
        ],
        loop_arcs: [
            DroneArc::ALWAYS_ON,
            DroneArc { cycle_s: 100, hold_s: 30 },
            DroneArc::ALWAYS_ON,
            DroneArc { cycle_s: 60, hold_s: 20 },
        ],
        drone_tone: 1500,
        key: KeyChoice::Note(3),
        loop_transpose: 2,
        ..DEFAULT_SETTINGS
    };
    let h = hash(&render_with(DEFAULT_SEEDS, settings, SECONDS * SAMPLE_RATE as usize, 4096));
    assert_eq!(h, PHASE3_GOLDEN_HASH, "the phase-3 features changed");
}

#[test]
fn other_tempos_ignore_block_size() {
    let frames = 12 * SAMPLE_RATE as usize;
    for bpm in [70, 120, 180] {
        let settings = Settings { bpm, ..DEFAULT_SETTINGS };
        let reference = render_with(DEFAULT_SEEDS, settings, frames, 4096);
        for block in [1, 777] {
            assert!(
                render_with(DEFAULT_SEEDS, settings, frames, block) == reference,
                "{bpm} BPM, block {block} differs"
            );
        }
    }
}

#[test]
fn block_size_does_not_matter() {
    let frames = 10 * SAMPLE_RATE as usize;
    let reference = render(DEFAULT_SEEDS, frames, 128);
    for block in [1, 1000, 65536] {
        assert!(render(DEFAULT_SEEDS, frames, block) == reference, "block size {block} differs");
    }
}

#[test]
fn each_seed_changes_the_output() {
    let frames = 15 * SAMPLE_RATE as usize;
    let base = hash(&render(DEFAULT_SEEDS, frames, 4096));
    let variants = [
        Seeds { s1: 1, ..DEFAULT_SEEDS },
        Seeds { s2: 2, ..DEFAULT_SEEDS },
        Seeds { s3: 3, ..DEFAULT_SEEDS },
        Seeds { s4: 4, ..DEFAULT_SEEDS },
        Seeds { s5: 5, ..DEFAULT_SEEDS },
        Seeds { s6: 6, ..DEFAULT_SEEDS },
    ];
    for v in variants {
        assert_ne!(hash(&render(v, frames, 4096)), base, "{v:?} sounds the same as the default");
    }
}

#[test]
fn each_setting_changes_the_output() {
    // Long enough to reach the first chord change at half-time (16 bars ≈ 46 s).
    let frames = 48 * SAMPLE_RATE as usize;
    let base = hash(&render(DEFAULT_SEEDS, frames, 4096));
    let variants = [
        Settings { scale: Scale::Dorian, ..DEFAULT_SETTINGS },
        Settings { chords: 1, ..DEFAULT_SETTINGS },
        Settings { pace: Pace::Jungle, ..DEFAULT_SETTINGS },
        Settings { pace: Pace::Garage, ..DEFAULT_SETTINGS },
        Settings { kit: shrine0016::kits::Kit::FmMetal, ..DEFAULT_SETTINGS },
        Settings { bpm: 160, ..DEFAULT_SETTINGS },
        Settings { drone_tone: 1200, ..DEFAULT_SETTINGS },
        Settings { key: shrine0016::KeyChoice::Note(0), ..DEFAULT_SETTINGS },
        Settings { loop_transpose: 5, ..DEFAULT_SETTINGS },
        OLD_DEFAULTS,
    ];
    for v in variants {
        assert_ne!(hash(&render_with(DEFAULT_SEEDS, v, frames, 4096)), base, "{v:?} sounds the same as the default");
    }
}
