//! The track must render bit-identically forever. These tests lock that in.

use shrine0010::harmony::{Pace, Scale};
use shrine0010::{Seeds, Settings, Track, DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};

/// FNV-1a over the raw f32 bit patterns. `web/verify.mjs` computes the same
/// hash from the WASM build to prove native and browser output are identical.
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

fn render(seeds: Seeds, frames: usize, block: usize) -> Vec<f32> {
    render_with(seeds, DEFAULT_SETTINGS, frames, block)
}

fn render_with(seeds: Seeds, settings: Settings, frames: usize, block: usize) -> Vec<f32> {
    let mut track = Track::new(seeds, settings);
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(block * 2) {
        track.render(chunk);
    }
    out
}

const SECONDS: usize = 30;

/// The sound of the canonical track. If this changes, the music changed:
/// only update it deliberately, as a new version of the piece.
const GOLDEN_HASH: u64 = 0x2c58bac59719fe83;

#[test]
fn golden_hash() {
    let out = render(DEFAULT_SEEDS, SECONDS * SAMPLE_RATE as usize, 4096);
    let h = hash(&out);
    println!("golden hash: {h:#018x}");
    assert_eq!(h, GOLDEN_HASH, "the rendered audio changed (got {h:#018x})");
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
    ];
    for v in variants {
        assert_ne!(hash(&render(v, frames, 4096)), base, "{v:?} sounds the same as the default");
    }
}

#[test]
fn each_setting_changes_the_output() {
    // Long enough to reach the first chord change at half-time (16 bars ≈ 46 s):
    // before it, every chord count plays the same first chord.
    let frames = 48 * SAMPLE_RATE as usize;
    let base = hash(&render(DEFAULT_SEEDS, frames, 4096));
    let variants = [
        Settings { scale: Scale::Dorian, ..DEFAULT_SETTINGS },
        Settings { chords: 1, ..DEFAULT_SETTINGS },
        Settings { pace: Pace::Jungle, ..DEFAULT_SETTINGS },
        Settings { kit: shrine0010::kits::Kit::FmMetal, ..DEFAULT_SETTINGS },
    ];
    for v in variants {
        assert_ne!(hash(&render_with(DEFAULT_SEEDS, v, frames, 4096)), base, "{v:?} sounds the same as the default");
    }
}

/// The "Off" kit switches the drums off: no drum energy, ever, and the
/// drone is never ducked to make room for them.
#[test]
fn kit_off_is_silent_drums() {
    let off = Settings { kit: shrine0010::kits::Kit::Off, ..DEFAULT_SETTINGS };
    let mut track = Track::new(DEFAULT_SEEDS, off);
    assert!(!track.describe().drums_on);
    let mut buf = vec![0f32; 48_000 * 2];
    for _ in 0..60 {
        track.render(&mut buf);
        let s = track.snapshot();
        assert_eq!(s.meters.drums, 0.0);
        assert_eq!(s.drum_hit_count, 0);
        assert_eq!(s.duck_gain, 1.0);
    }
}

/// Seed 5 = 0 is an ordinary seed: with a kit chosen, the drums play.
#[test]
fn drum_seed_zero_still_plays() {
    let mut track = Track::new(Seeds { s5: 0, ..DEFAULT_SEEDS }, DEFAULT_SETTINGS);
    assert!(track.describe().drums_on);
    let mut buf = vec![0f32; 48_000 * 2];
    let mut hits = 0;
    for _ in 0..40 {
        track.render(&mut buf);
        hits = track.snapshot().drum_hit_count;
    }
    assert!(hits > 0);
}

/// Each of the six kit families makes a different break, and seed 5 varies
/// the break within a kit.
#[test]
fn every_kit_family_sounds_different() {
    use shrine0010::kits::KITS;
    let mut seen = Vec::new();
    for kit in KITS.into_iter().filter(|k| *k != shrine0010::kits::Kit::Off) {
        let d = Track::new(DEFAULT_SEEDS, Settings { kit, ..DEFAULT_SETTINGS }).describe();
        assert_eq!(d.drum_voices.kit, kit.name());
        assert!(!seen.contains(&d.break_overview), "{} repeats another kit's break", kit.name());
        seen.push(d.break_overview.clone());
        let other = Track::new(Seeds { s5: 7, ..DEFAULT_SEEDS }, Settings { kit, ..DEFAULT_SETTINGS }).describe();
        assert_eq!(other.drum_voices.kit, kit.name(), "seed 5 must not change the kit");
        assert_ne!(other.break_overview, d.break_overview, "seed 5 should vary the {} kit", kit.name());
    }
}

/// Two hours of playback: no NaN, no runaway feedback, never clipping.
/// Slow; run with `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn two_hours_stable() {
    let mut track = Track::new(DEFAULT_SEEDS, DEFAULT_SETTINGS);
    let mut buf = vec![0f32; 48_000 * 2];
    for minute_part in 0..(2 * 60 * 60) {
        track.render(&mut buf);
        let peak = buf.iter().fold(0f32, |a, s| a.max(s.abs()));
        assert!(buf.iter().all(|s| s.is_finite()), "non-finite at {minute_part} s");
        assert!(peak < 1.0, "peak {peak} at {minute_part} s");
        if minute_part > 60 {
            assert!(peak > 0.01, "silent at {minute_part} s");
        }
    }
}
